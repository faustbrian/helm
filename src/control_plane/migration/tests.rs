use super::{
    MigrationBackup, MigrationCutoverPlan, MigrationExecutionResult, MigrationFuture,
    MigrationOperations, MigrationRollbackPlan, confirm_migration, execute_migration,
    rollback_migration,
};
use crate::control_plane::state::{
    EnvironmentLifecycle, LogicalResourceRecord, LogicalResourceRecordOptions,
    ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions, MigrationPhase, MigrationRecord,
    MigrationRecordOptions, ProjectRecord, ResourceLifecycle, SqliteStateStore, StateStore,
};
use std::collections::BTreeMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn migration_executes_to_reversible_cutover_without_retiring_v7() {
    run_test(async {
        let database_path = temporary_database_path("execute");
        let mut store = migration_store(&database_path);
        let mut operations = RecordingMigrationOperations::default();

        let result = execute_migration(&mut store, &inventory(), &mut operations, 100)
            .await
            .expect("execute migration");

        assert_eq!(result, MigrationExecutionResult::AwaitingConfirmation);
        assert_eq!(
            operations.calls,
            ["backup", "provision", "restore", "verify", "cutover"]
        );
        assert_eq!(
            operations.received_phases,
            [
                MigrationPhase::Inventoried,
                MigrationPhase::BackupVerified,
                MigrationPhase::TargetProvisioned,
                MigrationPhase::DataRestored,
                MigrationPhase::TargetVerified,
            ]
        );
        assert_eq!(
            operations.restored_artifact_sha256.as_deref(),
            Some("sha256:backup")
        );
        assert_eq!(operations.restored_artifact_size_bytes, Some(1_024));
        let records = store.migrations().expect("load migration");
        assert_eq!(records[0].phase(), MigrationPhase::Cutover);
        assert_eq!(records[0].backup_reference(), Some("backup:bill/database"));
        assert_eq!(records[0].target_resource_id(), Some("postgres-v8-bill"));
        assert_eq!(
            records[0].rollback_reference(),
            Some("v7:container/database")
        );
        assert_eq!(
            store.projects().expect("load project"),
            vec![cutover_project()]
        );
        assert_eq!(
            store.managed_environments().expect("load environment"),
            vec![cutover_environment()]
        );

        drop(store);
        remove_database(&database_path);
    });
}

#[test]
fn migration_resumes_from_the_last_durable_checkpoint_after_failure() {
    run_test(async {
        let database_path = temporary_database_path("resume");
        let mut store = migration_store(&database_path);
        let mut failing = RecordingMigrationOperations {
            fail_restore: true,
            ..RecordingMigrationOperations::default()
        };

        let error = execute_migration(&mut store, &inventory(), &mut failing, 100)
            .await
            .expect_err("restore failure");

        assert_eq!(
            error.to_string(),
            "migration restore failed: target rejected restore"
        );
        assert_eq!(failing.calls, ["backup", "provision", "restore"]);
        assert_eq!(
            store.migrations().expect("failed checkpoint")[0].phase(),
            MigrationPhase::TargetProvisioned
        );

        let mut resumed = RecordingMigrationOperations::default();
        let result = execute_migration(&mut store, &inventory(), &mut resumed, 101)
            .await
            .expect("resume migration");

        assert_eq!(result, MigrationExecutionResult::AwaitingConfirmation);
        assert_eq!(resumed.calls, ["restore", "verify", "cutover"]);
        assert_eq!(
            resumed.restored_backup_reference.as_deref(),
            Some("backup:bill/database")
        );
        assert_eq!(
            resumed.restored_target_resource_id.as_deref(),
            Some("postgres-v8-bill")
        );

        drop(store);
        remove_database(&database_path);
    });
}

#[test]
fn confirmation_is_the_only_path_that_retires_the_v7_source() {
    run_test(async {
        let database_path = temporary_database_path("confirm");
        let mut store = migration_store(&database_path);
        execute_migration(
            &mut store,
            &inventory(),
            &mut RecordingMigrationOperations::default(),
            100,
        )
        .await
        .expect("execute migration");
        let mut confirmation = RecordingMigrationOperations::default();

        let result = confirm_migration(&mut store, &inventory(), &mut confirmation, 101)
            .await
            .expect("confirm migration");

        assert_eq!(result, MigrationExecutionResult::Confirmed);
        assert_eq!(confirmation.calls, ["retire"]);
        assert_eq!(
            store.migrations().expect("confirmed migration")[0].phase(),
            MigrationPhase::Confirmed
        );

        drop(store);
        remove_database(&database_path);
    });
}

#[test]
fn failed_cutover_retains_verified_target_and_replays_only_cutover() {
    run_test(async {
        let database_path = temporary_database_path("cutover-resume");
        let mut store = migration_store(&database_path);
        let mut failing = RecordingMigrationOperations {
            fail_cutover: true,
            ..RecordingMigrationOperations::default()
        };

        let error = execute_migration(&mut store, &inventory(), &mut failing, 100)
            .await
            .expect_err("cutover failure");

        assert_eq!(
            error.to_string(),
            "migration cutover failed: route swap failed"
        );
        assert_eq!(
            store.migrations().expect("verified checkpoint")[0].phase(),
            MigrationPhase::TargetVerified
        );
        assert_eq!(
            store.projects().expect("original project"),
            vec![original_project()]
        );
        assert_eq!(
            store.managed_environments().expect("original environment"),
            vec![original_environment()]
        );

        let mut resumed = RecordingMigrationOperations::default();
        execute_migration(&mut store, &inventory(), &mut resumed, 101)
            .await
            .expect("resume cutover");
        assert_eq!(resumed.calls, ["cutover"]);

        drop(store);
        remove_database(&database_path);
    });
}

#[test]
fn migration_backup_requires_complete_verified_evidence() {
    let error =
        MigrationBackup::new("", "sha256:backup", 1_024).expect_err("missing backup reference");

    assert_eq!(
        error.to_string(),
        "backup operation returned incomplete verified evidence"
    );
}

#[test]
fn explicit_rollback_retains_proof_and_becomes_terminal() {
    run_test(async {
        let database_path = temporary_database_path("rollback");
        let mut store = migration_store(&database_path);
        execute_migration(
            &mut store,
            &inventory(),
            &mut RecordingMigrationOperations::default(),
            100,
        )
        .await
        .expect("execute migration");
        let mut rollback = RecordingMigrationOperations::default();

        let result = rollback_migration(&mut store, &inventory(), &mut rollback, 101)
            .await
            .expect("roll back migration");

        assert_eq!(result, MigrationExecutionResult::RolledBack);
        assert_eq!(rollback.calls, ["rollback"]);
        let record = &store.migrations().expect("rolled back migration")[0];
        assert_eq!(record.phase(), MigrationPhase::RolledBack);
        assert_eq!(record.backup_reference(), Some("backup:bill/database"));
        assert_eq!(record.target_resource_id(), Some("postgres-v8-bill"));
        assert_eq!(
            store.projects().expect("rolled-back project"),
            vec![original_project()]
        );
        assert_eq!(
            store
                .managed_environments()
                .expect("rolled-back environment"),
            vec![original_environment()]
        );
        assert_eq!(
            store.logical_resources().expect("retained target"),
            vec![retained_target_logical_resource()]
        );

        drop(store);
        remove_database(&database_path);
    });
}

fn inventory() -> MigrationRecord {
    MigrationRecord::new(MigrationRecordOptions {
        migration_id: "migration-bill-database".to_owned(),
        project_id: "bill".to_owned(),
        source_revision: "sha256:v7".to_owned(),
        target_revision: "sha256:v8".to_owned(),
        source_compatibility_fingerprint: "sha256:postgres-16".to_owned(),
        target_compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        phase: MigrationPhase::Inventoried,
        backup_reference: None,
        backup_artifact_sha256: None,
        backup_artifact_size_bytes: None,
        target_resource_id: None,
        rollback_reference: Some("v7:container/database".to_owned()),
        updated_at_unix_seconds: 99,
    })
    .expect("valid migration plan")
}

fn original_project() -> ProjectRecord {
    ProjectRecord::new(
        PathBuf::from("/work/bill"),
        "bill".to_owned(),
        vec!["bill-app.stackctl.localhost".to_owned()],
    )
}

fn cutover_project() -> ProjectRecord {
    ProjectRecord::new(
        PathBuf::from("/work/bill"),
        "bill".to_owned(),
        vec![
            "bill-app.stackctl.localhost".to_owned(),
            "bill-mailpit.stackctl.localhost".to_owned(),
        ],
    )
}

fn original_environment() -> ManagedEnvironmentRecord {
    ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:environment-v7".to_owned(),
        values: BTreeMap::from([("DB_DATABASE".to_owned(), "legacy_bill".to_owned())]),
        lifecycle: EnvironmentLifecycle::Active,
    })
}

fn cutover_environment() -> ManagedEnvironmentRecord {
    ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:environment-v8".to_owned(),
        values: BTreeMap::from([(
            "DB_DATABASE".to_owned(),
            "stackctl_bill_database".to_owned(),
        )]),
        lifecycle: EnvironmentLifecycle::Active,
    })
}

fn active_target_logical_resource() -> LogicalResourceRecord {
    target_logical_resource(ResourceLifecycle::Active)
}

fn retained_target_logical_resource() -> LogicalResourceRecord {
    target_logical_resource(ResourceLifecycle::Retained)
}

fn target_logical_resource(lifecycle: ResourceLifecycle) -> LogicalResourceRecord {
    LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/database".to_owned(),
        shared_resource_id: "postgres-shared-17".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        desired_revision: "sha256:v8".to_owned(),
        lifecycle,
        orphaned_at_unix_seconds: None,
    })
}

fn migration_store(database_path: &Path) -> SqliteStateStore {
    let mut store = SqliteStateStore::open(database_path).expect("open state");
    store
        .replace_project(&original_project())
        .expect("persist original project");
    store
        .replace_managed_environment(&original_environment())
        .expect("persist original environment");
    store
        .upsert_logical_resources(&[active_target_logical_resource()])
        .expect("persist active target logical resource");

    store
}

#[derive(Default)]
struct RecordingMigrationOperations {
    calls: Vec<&'static str>,
    received_phases: Vec<MigrationPhase>,
    fail_restore: bool,
    fail_cutover: bool,
    restored_backup_reference: Option<String>,
    restored_target_resource_id: Option<String>,
    restored_artifact_sha256: Option<String>,
    restored_artifact_size_bytes: Option<u64>,
}

impl MigrationOperations for RecordingMigrationOperations {
    fn backup<'operation>(
        &'operation mut self,
        migration: &'operation MigrationRecord,
    ) -> MigrationFuture<'operation, MigrationBackup> {
        self.calls.push("backup");
        self.received_phases.push(migration.phase());
        Box::pin(async { MigrationBackup::new("backup:bill/database", "sha256:backup", 1_024) })
    }

    fn provision_target<'operation>(
        &'operation mut self,
        migration: &'operation MigrationRecord,
    ) -> MigrationFuture<'operation, String> {
        self.calls.push("provision");
        self.received_phases.push(migration.phase());
        Box::pin(async { Ok("postgres-v8-bill".to_owned()) })
    }

    fn restore<'operation>(
        &'operation mut self,
        migration: &'operation MigrationRecord,
        backup_reference: &'operation str,
        target_resource_id: &'operation str,
    ) -> MigrationFuture<'operation, ()> {
        self.calls.push("restore");
        self.received_phases.push(migration.phase());
        self.restored_backup_reference = Some(backup_reference.to_owned());
        self.restored_target_resource_id = Some(target_resource_id.to_owned());
        self.restored_artifact_sha256 = migration.backup_artifact_sha256().map(str::to_owned);
        self.restored_artifact_size_bytes = migration.backup_artifact_size_bytes();
        let fail = self.fail_restore;
        Box::pin(async move {
            if fail {
                Err(super::MigrationOperationError::new(
                    "target rejected restore",
                ))
            } else {
                Ok(())
            }
        })
    }

    fn verify_target<'operation>(
        &'operation mut self,
        migration: &'operation MigrationRecord,
        _target_resource_id: &'operation str,
    ) -> MigrationFuture<'operation, ()> {
        self.calls.push("verify");
        self.received_phases.push(migration.phase());
        Box::pin(async { Ok(()) })
    }

    fn plan_cutover<'operation>(
        &'operation mut self,
        migration: &'operation MigrationRecord,
        _target_resource_id: &'operation str,
        _rollback_reference: &'operation str,
    ) -> MigrationFuture<'operation, MigrationCutoverPlan> {
        self.calls.push("cutover");
        self.received_phases.push(migration.phase());
        let fail = self.fail_cutover;
        Box::pin(async move {
            if fail {
                Err(super::MigrationOperationError::new("route swap failed"))
            } else {
                MigrationCutoverPlan::new(cutover_project(), cutover_environment())
            }
        })
    }

    fn plan_rollback<'operation>(
        &'operation mut self,
        _inventory: &'operation MigrationRecord,
        _checkpoint: &'operation MigrationRecord,
    ) -> MigrationFuture<'operation, MigrationRollbackPlan> {
        self.calls.push("rollback");
        Box::pin(async {
            MigrationRollbackPlan::new(
                original_project(),
                original_environment(),
                vec![retained_target_logical_resource()],
            )
        })
    }

    fn retire_source<'operation>(
        &'operation mut self,
        _inventory: &'operation MigrationRecord,
        _checkpoint: &'operation MigrationRecord,
    ) -> MigrationFuture<'operation, ()> {
        self.calls.push("retire");
        Box::pin(async { Ok(()) })
    }
}

fn temporary_database_path(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos();

    std::env::temp_dir().join(format!(
        "stackctl-migration-{name}-{}-{unique}.sqlite3",
        std::process::id()
    ))
}

fn remove_database(database_path: &Path) {
    for suffix in ["", "-shm", "-wal"] {
        let path = PathBuf::from(format!("{}{suffix}", database_path.display()));
        if path.exists() {
            std::fs::remove_file(path).expect("remove migration state database");
        }
    }
}

fn run_test<T>(future: impl Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("build migration test runtime")
        .block_on(future)
}
