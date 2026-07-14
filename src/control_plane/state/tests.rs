use super::{
    AcceptedV7EnvironmentRollback, AcceptedV7InventoryRecord, AcceptedV7InventoryRecordOptions,
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, DaemonEventRecord,
    EngineProvider, EnvironmentLifecycle, InstallationRecord, LogicalResourceRecord,
    LogicalResourceRecordOptions, ManagedEnvironmentRecord, ManagedEnvironmentRecordOptions,
    MigrationPhase, MigrationRecord, MigrationRecordOptions, ProjectAdoptionPlan,
    ProjectAdoptionPlanOptions, ProjectRecord, RecoveryPointRecord, RecoveryPointRecordOptions,
    ResourceLifecycle, ResourceRecord, ResourceRecordOptions, ResourceRetention, SqliteStateStore,
    StateStore, V7MigrationAdapterCheckpoint, V7MigrationExecutionPhase,
    V7MigrationExecutionRecord, V7MigrationExecutionRecordOptions,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn opening_a_new_store_applies_the_current_schema_atomically() {
    let database_path = temporary_database_path("migration");

    let store = SqliteStateStore::open(&database_path).expect("open state store");

    assert_eq!(store.schema_version().expect("schema version"), 18);
    assert_eq!(store.journal_mode().expect("journal mode"), "wal");

    drop(store);
    remove_database(&database_path);
}

#[test]
fn accepted_v7_inventory_is_append_only_idempotent_and_path_scoped() {
    let database_path = temporary_database_path("accepted-v7-inventory");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    let accepted = accepted_v7_inventory("a", 40_000);

    store
        .record_accepted_v7_inventory(&accepted)
        .expect("record accepted inventory");
    store
        .record_accepted_v7_inventory(&accepted)
        .expect("idempotent replay");

    assert_eq!(
        store
            .accepted_v7_inventory(Path::new("/work/bill"), accepted.evidence_revision())
            .expect("load accepted inventory"),
        Some(accepted)
    );
    assert!(
        store
            .latest_accepted_v7_inventory(Path::new("/work/other"))
            .expect("other project inventory")
            .is_none()
    );
    let replacement = accepted_v7_inventory("b", 40_001);
    store
        .record_accepted_v7_inventory(&replacement)
        .expect("append newly accepted evidence");
    assert_eq!(
        store
            .latest_accepted_v7_inventory(Path::new("/work/bill"))
            .expect("latest accepted inventory"),
        Some(replacement)
    );
    let collision = accepted_v7_inventory_at("bill", "/work/bill-copy", "c", 40_002);
    let error = store
        .record_accepted_v7_inventory(&collision)
        .expect_err("duplicate project identity must fail loudly");
    assert!(error.to_string().contains("already accepted at"));

    drop(store);
    remove_database(&database_path);
}

#[test]
fn accepted_v7_environment_requires_complete_protected_rollback_evidence() {
    let database_path = temporary_database_path("accepted-v7-environment-rollback");
    let source_revision = format!("sha256:{}", "a".repeat(64));
    let inventory_json = format!(
        r#"{{"project_id":"bill","canonical_project_path":"/work/bill","source_revision":"{source_revision}","blockers":[],"host_artifacts":{{"generated_environment":{{"path":"/work/bill/.env"}}}}}}"#,
    );
    let evidence_revision = hex::encode(Sha256::digest(inventory_json.as_bytes()));
    let legacy = rusqlite::Connection::open(&database_path).expect("legacy state database");
    legacy
        .execute_batch(
            "CREATE TABLE accepted_v7_inventories (
                 canonical_project_path TEXT NOT NULL,
                 project_id TEXT NOT NULL CHECK(length(project_id) > 0),
                 source_revision TEXT NOT NULL CHECK(length(source_revision) = 71),
                 evidence_revision TEXT NOT NULL CHECK(length(evidence_revision) = 64),
                 inventory_json TEXT NOT NULL CHECK(length(inventory_json) > 0),
                 accepted_at_unix_seconds INTEGER NOT NULL
                     CHECK(accepted_at_unix_seconds >= 0),
                 PRIMARY KEY(canonical_project_path, evidence_revision)
             ) STRICT;
             CREATE INDEX accepted_v7_inventories_project_idx
                 ON accepted_v7_inventories(project_id, canonical_project_path);
             PRAGMA user_version = 16;",
        )
        .expect("legacy accepted inventory schema");
    legacy
        .execute(
            "INSERT INTO accepted_v7_inventories (
                 canonical_project_path, project_id, source_revision,
                 evidence_revision, inventory_json, accepted_at_unix_seconds
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                "/work/bill",
                "bill",
                source_revision,
                evidence_revision,
                inventory_json,
                40_000_i64,
            ],
        )
        .expect("legacy accepted inventory");
    drop(legacy);
    let mut store = SqliteStateStore::open(&database_path).expect("upgrade state store");

    let incomplete = store
        .accepted_v7_inventory(Path::new("/work/bill"), &evidence_revision)
        .expect("load schema-16 accepted inventory")
        .expect("legacy accepted inventory exists");
    assert!(incomplete.requires_generated_environment_rollback());
    let error = store
        .record_accepted_v7_inventory(&incomplete)
        .expect_err("new acceptance requires protected environment rollback");
    assert!(error.to_string().contains("differs from durable evidence"));

    let rollback = AcceptedV7EnvironmentRollback::new(
        PathBuf::from("/private/backups/bill/environment"),
        "b".repeat(64),
        128,
    )
    .expect("environment rollback evidence");
    let accepted = AcceptedV7InventoryRecord::new(AcceptedV7InventoryRecordOptions {
        project_id: "bill".to_owned(),
        canonical_project_path: PathBuf::from("/work/bill"),
        source_revision: source_revision.clone(),
        inventory_json: inventory_json.clone(),
        generated_environment_rollback: Some(rollback.clone()),
        accepted_at_unix_seconds: 40_000,
    })
    .expect("accepted inventory with environment rollback");
    store
        .record_accepted_v7_inventory(&accepted)
        .expect("enrich accepted inventory with protected rollback");
    let restored = store
        .accepted_v7_inventory(Path::new("/work/bill"), accepted.evidence_revision())
        .expect("load accepted inventory")
        .expect("accepted inventory exists");

    assert_eq!(restored.generated_environment_rollback(), Some(&rollback));
    assert_eq!(restored.accepted_at_unix_seconds(), 40_000);

    drop(store);
    remove_database(&database_path);
}

#[test]
fn beginning_installation_deletion_atomically_freezes_and_orphans_projects() {
    let database_path = temporary_database_path("installation-deletion");
    let project = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let resource = resource_record("container-bill", "bill", ResourceRetention::Persistent);
    let logical = logical_resource_record("bill/database", "bill", "database");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .initialize_installation(&InstallationRecord::new(
            "install-1",
            EngineProvider::Docker,
            "unix:///engine.sock",
        ))
        .expect("installation identity");
    store
        .replace_watched_roots(&[PathBuf::from("/work")])
        .expect("watched root");
    store.replace_project(&project).expect("project state");
    store
        .upsert_resources(std::slice::from_ref(&resource))
        .expect("physical resource");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("logical resource");
    store
        .insert_credential_if_absent(&credential_record("secret-first"))
        .expect("credential");
    store
        .replace_managed_environment(&managed_environment(BTreeMap::new()))
        .expect("environment");

    store
        .begin_installation_deletion(70_000)
        .expect("begin deletion");
    store
        .begin_installation_deletion(70_000)
        .expect("idempotent replay");

    assert_eq!(
        store
            .installation_lifecycle()
            .expect("installation lifecycle"),
        Some(super::InstallationLifecycle::Deleting)
    );
    assert!(store.watched_roots().expect("watched roots").is_empty());
    assert!(store.projects().expect("projects").is_empty());
    assert_eq!(
        store.resources().expect("resources")[0].lifecycle(),
        ResourceLifecycle::Orphaned
    );
    assert_eq!(
        store.logical_resources().expect("logical resources")[0].lifecycle(),
        ResourceLifecycle::Orphaned
    );
    assert_eq!(
        store.credentials().expect("credentials")[0].lifecycle(),
        CredentialLifecycle::Disabled
    );
    assert_eq!(
        store.managed_environments().expect("environments")[0].lifecycle(),
        EnvironmentLifecycle::Disabled
    );
    let error = store
        .replace_project(&project)
        .expect_err("deleting installation must not reactivate projects");
    assert!(
        error
            .to_string()
            .contains("project reconciliation is frozen")
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn completing_installation_deletion_requires_all_logical_tenants_retired() {
    let database_path = temporary_database_path("installation-deletion-complete");
    let logical = logical_resource_record("bill/database", "bill", "database");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .initialize_installation(&InstallationRecord::new(
            "install-1",
            EngineProvider::Docker,
            "unix:///engine.sock",
        ))
        .expect("installation identity");
    store
        .replace_project(&project_record("/work/bill", "bill", &[]))
        .expect("project state");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("logical resource");
    store
        .insert_credential_if_absent(&credential_record("secret-first"))
        .expect("credential");
    store
        .begin_installation_deletion(70_000)
        .expect("begin deletion");

    let error = store
        .complete_installation_deletion()
        .expect_err("live retained tenant must block completion");
    assert!(error.to_string().contains("logical resources remain"));
    let logical = store.logical_resources().expect("orphaned logical")[0].clone();
    let credential = store.credentials().expect("disabled credential")[0].clone();
    store
        .retire_logical_resource(&logical, &credential)
        .expect("retire logical tenant");
    store
        .complete_installation_deletion()
        .expect("complete deletion");

    assert_eq!(
        store
            .installation_lifecycle()
            .expect("installation lifecycle"),
        Some(super::InstallationLifecycle::Deleted)
    );
    assert!(store.resources().expect("resources cleared").is_empty());
    assert!(store.credentials().expect("credentials cleared").is_empty());
    let error = store
        .begin_installation_deletion(70_001)
        .expect_err("deleted installation must not restart teardown");
    assert!(error.to_string().contains("terminally deleted"));
    assert_eq!(
        store.installation_lifecycle().expect("terminal lifecycle"),
        Some(super::InstallationLifecycle::Deleted)
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn recovery_points_are_immutable_idempotent_and_project_scoped() {
    let database_path = temporary_database_path("recovery-points");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    let point = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "backup-42".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        logical_resource_id: "stackctl_bill_database".to_owned(),
        resource_kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        reference: "/state/backups/backup-42".to_owned(),
        artifact_sha256: "a".repeat(64),
        artifact_size_bytes: 1_024,
        created_at_unix_seconds: 40_000,
        verified_at_unix_seconds: 40_001,
    })
    .expect("valid recovery point");

    store
        .record_recovery_point(&point)
        .expect("record recovery point");
    store
        .record_recovery_point(&point)
        .expect("idempotent replay");

    assert_eq!(
        store
            .recovery_points("bill")
            .expect("project recovery points"),
        vec![point.clone()]
    );
    assert!(
        store
            .recovery_points("other")
            .expect("other project recovery points")
            .is_empty()
    );
    let replacement = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "backup-42".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        logical_resource_id: "stackctl_bill_database".to_owned(),
        resource_kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        reference: "/state/backups/backup-42".to_owned(),
        artifact_sha256: "b".repeat(64),
        artifact_size_bytes: 1_024,
        created_at_unix_seconds: 40_000,
        verified_at_unix_seconds: 40_001,
    })
    .expect("valid conflicting recovery point");
    let error = store
        .record_recovery_point(&replacement)
        .expect_err("immutable evidence must not change");
    assert!(error.to_string().contains("immutable evidence"));

    drop(store);
    remove_database(&database_path);
}

#[test]
fn daemon_event_journal_is_bounded_and_monotonic_across_restarts() {
    let database_path = temporary_database_path("daemon-events");

    {
        let mut store = SqliteStateStore::open(&database_path).expect("open state store");
        for (operation_id, kind_json) in [
            ("operation-1", r#"{"type":"accepted"}"#),
            ("operation-1", r#"{"type":"completed"}"#),
            (
                "operation-2",
                r#"{"type":"failed","code":"blocked","message":"bounded"}"#,
            ),
        ] {
            store
                .append_daemon_event(operation_id, kind_json, 2)
                .expect("append daemon event");
        }

        assert_eq!(
            store.daemon_events().expect("load retained daemon events"),
            vec![
                DaemonEventRecord::new(
                    2,
                    "operation-1".to_owned(),
                    r#"{"type":"completed"}"#.to_owned(),
                ),
                DaemonEventRecord::new(
                    3,
                    "operation-2".to_owned(),
                    r#"{"type":"failed","code":"blocked","message":"bounded"}"#.to_owned(),
                ),
            ]
        );
    }

    let mut store = SqliteStateStore::open(&database_path).expect("reopen state store");
    let appended = store
        .append_daemon_event("operation-3", r#"{"type":"accepted"}"#, 2)
        .expect("append after restart");
    assert_eq!(appended.sequence(), 4);
    assert_eq!(
        store
            .daemon_events()
            .expect("load events after restart")
            .iter()
            .map(DaemonEventRecord::sequence)
            .collect::<Vec<_>>(),
        vec![3, 4]
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn migration_journal_survives_restart_and_advances_without_skips() {
    let database_path = temporary_database_path("migration-journal");

    {
        let mut store = SqliteStateStore::open(&database_path).expect("open state store");
        store
            .record_migration(&migration_record(MigrationPhase::Inventoried, 10))
            .expect("record inventory");
        store
            .record_migration(&migration_record(MigrationPhase::BackupVerified, 11))
            .expect("record backup");
        store
            .record_migration(&migration_record(MigrationPhase::TargetProvisioned, 12))
            .expect("record target");
    }

    let store = SqliteStateStore::open(&database_path).expect("reopen state store");

    assert_eq!(
        store.migrations().expect("load migrations"),
        vec![migration_record(MigrationPhase::TargetProvisioned, 12)]
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn migration_journal_rejects_skips_identity_drift_and_terminal_changes() {
    let database_path = temporary_database_path("migration-transitions");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .record_migration(&migration_record(MigrationPhase::Inventoried, 10))
        .expect("record inventory");

    let skipped = store
        .record_migration(&migration_record(MigrationPhase::TargetProvisioned, 12))
        .expect_err("reject skipped backup");
    assert_eq!(
        skipped.to_string(),
        "migration 'migration-bill' cannot advance from 'inventoried' to 'target_provisioned'"
    );

    let mut drifted = migration_options(MigrationPhase::BackupVerified, 11);
    drifted.target_revision = "sha256:different".to_owned();
    let drifted = MigrationRecord::new(drifted).expect("valid drifted record");
    let drift = store
        .record_migration(&drifted)
        .expect_err("reject migration identity drift");
    assert_eq!(
        drift.to_string(),
        "migration 'migration-bill' immutable identity differs from durable state"
    );

    store
        .record_migration(&migration_record(MigrationPhase::BackupVerified, 11))
        .expect("record verified backup");
    let mut replaced_evidence = migration_options(MigrationPhase::TargetProvisioned, 12);
    replaced_evidence.backup_artifact_sha256 = Some("sha256:replacement".to_owned());
    let replaced_evidence = MigrationRecord::new(replaced_evidence).expect("valid replacement");
    let evidence = store
        .record_migration(&replaced_evidence)
        .expect_err("reject replaced backup evidence");
    assert_eq!(
        evidence.to_string(),
        "migration 'migration-bill' durable evidence cannot be replaced"
    );

    let stale = store
        .record_migration(&migration_record(MigrationPhase::TargetProvisioned, 9))
        .expect_err("reject stale checkpoint");
    assert_eq!(
        stale.to_string(),
        "migration 'migration-bill' update time predates durable state"
    );

    for (phase, updated_at) in [
        (MigrationPhase::TargetProvisioned, 12),
        (MigrationPhase::DataRestored, 13),
        (MigrationPhase::TargetVerified, 14),
        (MigrationPhase::Cutover, 15),
        (MigrationPhase::Confirmed, 16),
    ] {
        store
            .record_migration(&migration_record(phase, updated_at))
            .expect("advance migration");
    }

    let terminal = store
        .record_migration(&migration_record(MigrationPhase::RolledBack, 17))
        .expect_err("reject terminal transition");
    assert_eq!(
        terminal.to_string(),
        "migration 'migration-bill' cannot advance from 'confirmed' to 'rolled_back'"
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn migration_records_require_proof_before_destructive_phases() {
    let mut options = migration_options(MigrationPhase::BackupVerified, 11);
    options.backup_artifact_sha256 = None;
    options.backup_artifact_size_bytes = None;
    let backup_error = MigrationRecord::new(options).expect_err("missing backup proof");
    assert_eq!(
        backup_error.to_string(),
        "migration phase 'backup_verified' requires verified backup evidence"
    );

    let mut options = migration_options(MigrationPhase::TargetProvisioned, 12);
    options.target_resource_id = None;
    let target_error = MigrationRecord::new(options).expect_err("missing target identity");
    assert_eq!(
        target_error.to_string(),
        "migration phase 'target_provisioned' requires a target resource identity"
    );

    let mut options = migration_options(MigrationPhase::Cutover, 15);
    options.rollback_reference = None;
    let rollback_error = MigrationRecord::new(options).expect_err("missing rollback material");
    assert_eq!(
        rollback_error.to_string(),
        "migration phase 'cutover' requires retained rollback material"
    );
}

#[test]
fn v7_migration_execution_records_require_a_complete_verified_barrier() {
    let mutating =
        V7MigrationAdapterCheckpoint::pending("service/app", "recreate-project-workload", true, 10)
            .expect("pending mutating adapter");
    let no_op = V7MigrationAdapterCheckpoint::pending("route", "no-routes", false, 10)
        .expect("pending no-op adapter");
    let error = V7MigrationExecutionRecord::new(v7_execution_options(
        V7MigrationExecutionPhase::Prepared,
        vec![mutating.clone(), no_op.clone()],
        10,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ))
    .expect_err("unverified prepared barrier");
    assert_eq!(
        error,
        "v7 migration execution phase 'prepared' requires every adapter target to be verified"
    );

    let mutating = mutating
        .with_recovery_verified(
            "/backups/app",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            100,
            11,
        )
        .expect("verified recovery")
        .with_target_verified(Some("container-app"), 12)
        .expect("verified target");
    let no_op = no_op
        .with_target_verified(None, 12)
        .expect("verified no-op");
    let prepared = V7MigrationExecutionRecord::new(v7_execution_options(
        V7MigrationExecutionPhase::Prepared,
        vec![mutating, no_op],
        12,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    ))
    .expect("complete prepared barrier");

    assert_eq!(prepared.checkpoints().len(), 2);
}

#[test]
fn v7_migration_execution_journal_is_atomic_monotonic_and_restart_safe() {
    let database_path = temporary_database_path("v7-migration-execution");
    let accepted = accepted_v7_inventory("a", 9);
    let pending =
        V7MigrationAdapterCheckpoint::pending("service/app", "recreate-project-workload", true, 10)
            .expect("pending adapter");
    let planned = V7MigrationExecutionRecord::new(v7_execution_options(
        V7MigrationExecutionPhase::Planned,
        vec![pending.clone()],
        10,
        accepted.evidence_revision(),
    ))
    .expect("planned execution");
    let recovery = pending
        .with_recovery_verified(
            "/backups/app",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            100,
            11,
        )
        .expect("verified recovery");
    let preparing = V7MigrationExecutionRecord::new(v7_execution_options(
        V7MigrationExecutionPhase::Preparing,
        vec![recovery.clone()],
        11,
        accepted.evidence_revision(),
    ))
    .expect("preparing execution");
    let target = recovery
        .with_target_verified(Some("container-app"), 12)
        .expect("verified target");
    let prepared = V7MigrationExecutionRecord::new(v7_execution_options(
        V7MigrationExecutionPhase::Prepared,
        vec![target],
        12,
        accepted.evidence_revision(),
    ))
    .expect("prepared execution");

    {
        let mut store = SqliteStateStore::open(&database_path).expect("open state store");
        store
            .record_accepted_v7_inventory(&accepted)
            .expect("record accepted source");
        store
            .record_v7_migration_execution(&planned)
            .expect("record planned execution");
        store
            .record_v7_migration_execution(&preparing)
            .expect("record preparing execution");
        let skipped = V7MigrationExecutionRecord::new(v7_execution_options(
            V7MigrationExecutionPhase::Cutover,
            vec![
                prepared.checkpoints()[0]
                    .clone()
                    .with_cutover(13)
                    .expect("cutover adapter"),
            ],
            13,
            accepted.evidence_revision(),
        ))
        .expect("structurally valid cutover");
        assert_eq!(
            store
                .record_v7_migration_execution(&skipped)
                .expect_err("prepared barrier cannot be skipped")
                .to_string(),
            "v7 migration execution for '/work/bill' cannot advance from 'preparing' to 'cutover'"
        );
        store
            .record_v7_migration_execution(&prepared)
            .expect("record prepared execution");
    }

    let store = SqliteStateStore::open(&database_path).expect("reopen state store");
    assert_eq!(
        store
            .v7_migration_execution(Path::new("/work/bill"), accepted.evidence_revision(),)
            .expect("load execution"),
        Some(prepared)
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn v7_cutover_atomically_publishes_project_environment_and_execution() {
    let database_path = temporary_database_path("v7-cutover-transaction");
    let accepted = accepted_v7_inventory("a", 9);
    let original_project =
        project_record("/work/bill", "bill", &["bill-legacy.stackctl.localhost"]);
    let cutover_project = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let original_environment = managed_environment(BTreeMap::from([(
        "APP_STAGE".to_owned(),
        "legacy".to_owned(),
    )]));
    let cutover_environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:v8-environment".to_owned(),
        values: BTreeMap::from([("APP_STAGE".to_owned(), "v8".to_owned())]),
        lifecycle: EnvironmentLifecycle::Active,
    });
    let pending = V7MigrationAdapterCheckpoint::pending(
        "environment",
        "protected-generated-environment",
        false,
        10,
    )
    .expect("pending adapter");
    let target = pending
        .clone()
        .with_target_verified(Some("managed-environment:sha256:v8-environment"), 11)
        .expect("verified target");
    let planned = V7MigrationExecutionRecord::new(v7_execution_options(
        V7MigrationExecutionPhase::Planned,
        vec![pending],
        10,
        accepted.evidence_revision(),
    ))
    .expect("planned execution");
    let prepared = V7MigrationExecutionRecord::new(v7_execution_options(
        V7MigrationExecutionPhase::Prepared,
        vec![target.clone()],
        11,
        accepted.evidence_revision(),
    ))
    .expect("prepared execution");
    let preparing = V7MigrationExecutionRecord::new(v7_execution_options(
        V7MigrationExecutionPhase::Preparing,
        vec![target.clone()],
        11,
        accepted.evidence_revision(),
    ))
    .expect("preparing execution");
    let cutover = V7MigrationExecutionRecord::new(v7_execution_options(
        V7MigrationExecutionPhase::Cutover,
        vec![target.with_cutover(12).expect("cutover adapter")],
        12,
        accepted.evidence_revision(),
    ))
    .expect("cutover execution");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .record_accepted_v7_inventory(&accepted)
        .expect("record accepted source");
    store
        .replace_project(&original_project)
        .expect("record original project");
    store
        .replace_managed_environment(&original_environment)
        .expect("record original environment");
    store
        .record_v7_migration_execution(&planned)
        .expect("record planned execution");

    store
        .record_v7_migration_cutover(&cutover_project, &cutover_environment, &cutover)
        .expect_err("cannot skip prepared barrier");
    assert_eq!(
        store.projects().expect("load original project"),
        vec![original_project]
    );
    assert_eq!(
        store
            .managed_environments()
            .expect("load original environment"),
        vec![original_environment]
    );
    assert_eq!(
        store
            .v7_migration_execution(Path::new("/work/bill"), accepted.evidence_revision())
            .expect("load planned execution"),
        Some(planned)
    );

    store
        .record_v7_migration_execution(&preparing)
        .expect("record preparing execution");
    store
        .record_v7_migration_execution(&prepared)
        .expect("record prepared execution");
    store
        .record_v7_migration_cutover(&cutover_project, &cutover_environment, &cutover)
        .expect("record atomic cutover");
    assert_eq!(
        store.projects().expect("load cutover project"),
        vec![cutover_project]
    );
    assert_eq!(
        store
            .managed_environments()
            .expect("load cutover environment"),
        vec![cutover_environment]
    );
    assert_eq!(
        store
            .v7_migration_execution(Path::new("/work/bill"), accepted.evidence_revision())
            .expect("load cutover execution"),
        Some(cutover)
    );

    drop(store);
    remove_database(&database_path);
}

fn v7_execution_options(
    phase: V7MigrationExecutionPhase,
    checkpoints: Vec<V7MigrationAdapterCheckpoint>,
    updated_at_unix_seconds: i64,
    evidence_revision: &str,
) -> V7MigrationExecutionRecordOptions {
    V7MigrationExecutionRecordOptions {
        project_id: "bill".to_owned(),
        canonical_project_path: PathBuf::from("/work/bill"),
        evidence_revision: evidence_revision.to_owned(),
        adapter_plan_revision: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            .to_owned(),
        phase,
        checkpoints,
        updated_at_unix_seconds,
    }
}

#[test]
fn rejected_migration_target_ownership_rolls_back_logical_state_and_checkpoint() {
    let database_path = temporary_database_path("migration-target-transaction");
    let project = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let target = logical_resource_record("bill/database", "bill", "database");
    let stable_credential = credential_record("stable-secret");
    let conflicting_credential = credential_record("replacement-secret");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store.replace_project(&project).expect("persist project");
    store
        .insert_credential_if_absent(&stable_credential)
        .expect("persist stable credential");
    store
        .record_migration(&migration_record(MigrationPhase::Inventoried, 10))
        .expect("record inventory");
    store
        .record_migration(&migration_record(MigrationPhase::BackupVerified, 11))
        .expect("record verified backup");

    let error = store
        .record_migration_target(
            &target,
            &conflicting_credential,
            &migration_record(MigrationPhase::TargetProvisioned, 12),
        )
        .expect_err("reject changed target credential");

    assert_eq!(
        error.to_string(),
        "migration target credential 'bill/database/primary' differs from durable state"
    );
    assert!(
        store
            .logical_resources()
            .expect("no partial logical target")
            .is_empty()
    );
    assert_eq!(
        store.credentials().expect("stable credential"),
        vec![stable_credential]
    );
    assert_eq!(
        store.migrations().expect("verified checkpoint")[0].phase(),
        MigrationPhase::BackupVerified
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn migration_cutover_replaces_desired_state_and_checkpoint_atomically() {
    let database_path = temporary_database_path("migration-cutover");
    let original_project = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let original_environment = managed_environment(BTreeMap::from([(
        "DB_DATABASE".to_owned(),
        "legacy_bill".to_owned(),
    )]));
    let cutover_project = project_record(
        "/work/bill",
        "bill",
        &[
            "bill-app.stackctl.localhost",
            "bill-mailpit.stackctl.localhost",
        ],
    );
    let cutover_environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:environment-v8".to_owned(),
        values: BTreeMap::from([(
            "DB_DATABASE".to_owned(),
            "stackctl_bill_database".to_owned(),
        )]),
        lifecycle: EnvironmentLifecycle::Active,
    });
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .replace_project(&original_project)
        .expect("persist original project");
    store
        .replace_managed_environment(&original_environment)
        .expect("persist original environment");
    for (phase, updated_at) in [
        (MigrationPhase::Inventoried, 10),
        (MigrationPhase::BackupVerified, 11),
        (MigrationPhase::TargetProvisioned, 12),
        (MigrationPhase::DataRestored, 13),
        (MigrationPhase::TargetVerified, 14),
    ] {
        store
            .record_migration(&migration_record(phase, updated_at))
            .expect("advance migration to verified target");
    }

    store
        .record_migration_cutover(
            &cutover_project,
            &cutover_environment,
            &migration_record(MigrationPhase::Cutover, 15),
        )
        .expect("commit migration cutover");

    assert_eq!(
        store.projects().expect("load project"),
        vec![cutover_project]
    );
    assert_eq!(
        store
            .managed_environments()
            .expect("load managed environment"),
        vec![cutover_environment]
    );
    assert_eq!(
        store.migrations().expect("load migration")[0].phase(),
        MigrationPhase::Cutover
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn rejected_migration_cutover_leaves_all_desired_state_unchanged() {
    let database_path = temporary_database_path("migration-cutover-rollback");
    let original_project = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let original_environment = managed_environment(BTreeMap::from([(
        "DB_DATABASE".to_owned(),
        "legacy_bill".to_owned(),
    )]));
    let cutover_project = project_record(
        "/work/bill",
        "bill",
        &[
            "bill-app.stackctl.localhost",
            "bill-mailpit.stackctl.localhost",
        ],
    );
    let cutover_environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:environment-v8".to_owned(),
        values: BTreeMap::from([(
            "DB_DATABASE".to_owned(),
            "stackctl_bill_database".to_owned(),
        )]),
        lifecycle: EnvironmentLifecycle::Active,
    });
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .replace_project(&original_project)
        .expect("persist original project");
    store
        .replace_managed_environment(&original_environment)
        .expect("persist original environment");
    for (phase, updated_at) in [
        (MigrationPhase::Inventoried, 10),
        (MigrationPhase::BackupVerified, 11),
        (MigrationPhase::TargetProvisioned, 12),
    ] {
        store
            .record_migration(&migration_record(phase, updated_at))
            .expect("advance migration to provisioned target");
    }

    let error = store
        .record_migration_cutover(
            &cutover_project,
            &cutover_environment,
            &migration_record(MigrationPhase::Cutover, 15),
        )
        .expect_err("reject skipped migration phases");

    assert_eq!(
        error.to_string(),
        "migration 'migration-bill' cannot advance from 'target_provisioned' to 'cutover'"
    );
    assert_eq!(
        store.projects().expect("load original project"),
        vec![original_project]
    );
    assert_eq!(
        store
            .managed_environments()
            .expect("load original managed environment"),
        vec![original_environment]
    );
    assert_eq!(
        store.migrations().expect("load migration")[0].phase(),
        MigrationPhase::TargetProvisioned
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn rejected_migration_rollback_leaves_cutover_state_and_target_active() {
    let database_path = temporary_database_path("migration-rollback-transaction");
    let cutover_project = project_record(
        "/work/bill",
        "bill",
        &[
            "bill-app.stackctl.localhost",
            "bill-mailpit.stackctl.localhost",
        ],
    );
    let cutover_environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:environment-v8".to_owned(),
        values: BTreeMap::from([(
            "DB_DATABASE".to_owned(),
            "stackctl_bill_database".to_owned(),
        )]),
        lifecycle: EnvironmentLifecycle::Active,
    });
    let rollback_project = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let rollback_environment = managed_environment(BTreeMap::from([(
        "DB_DATABASE".to_owned(),
        "legacy_bill".to_owned(),
    )]));
    let active_target = logical_resource_record("bill/database", "bill", "database");
    let retained_target = logical_resource_record_with_lifecycle(
        "bill/database",
        "bill",
        "database",
        ResourceLifecycle::Retained,
    );
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .replace_project(&cutover_project)
        .expect("persist cutover project");
    store
        .replace_managed_environment(&cutover_environment)
        .expect("persist cutover environment");
    store
        .upsert_logical_resources(std::slice::from_ref(&active_target))
        .expect("persist active target");
    for (phase, updated_at) in [
        (MigrationPhase::Inventoried, 10),
        (MigrationPhase::BackupVerified, 11),
        (MigrationPhase::TargetProvisioned, 12),
        (MigrationPhase::DataRestored, 13),
        (MigrationPhase::TargetVerified, 14),
        (MigrationPhase::Cutover, 15),
    ] {
        store
            .record_migration(&migration_record(phase, updated_at))
            .expect("advance migration through cutover");
    }

    let error = store
        .record_migration_rollback(
            &rollback_project,
            &rollback_environment,
            &[retained_target],
            &migration_record(MigrationPhase::RolledBack, 14),
        )
        .expect_err("reject rollback checkpoint time regression");

    assert_eq!(
        error.to_string(),
        "migration 'migration-bill' update time predates durable state"
    );
    assert_eq!(
        store.projects().expect("load cutover project"),
        vec![cutover_project]
    );
    assert_eq!(
        store
            .managed_environments()
            .expect("load cutover environment"),
        vec![cutover_environment]
    );
    assert_eq!(
        store.logical_resources().expect("load active target"),
        vec![active_target]
    );
    assert_eq!(
        store.migrations().expect("load migration")[0].phase(),
        MigrationPhase::Cutover
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn installation_and_watched_roots_survive_store_restart() {
    let database_path = temporary_database_path("installation");
    let installation = InstallationRecord::new(
        "install-1",
        EngineProvider::Docker,
        "unix:///var/run/docker.sock",
    );

    {
        let mut store = SqliteStateStore::open(&database_path).expect("open state store");
        store
            .initialize_installation(&installation)
            .expect("initialize installation");
        store
            .replace_watched_roots(&[
                PathBuf::from("/work/zeta"),
                PathBuf::from("/work/alpha"),
                PathBuf::from("/work/alpha"),
            ])
            .expect("persist watched roots");
    }

    let store = SqliteStateStore::open(&database_path).expect("reopen state store");

    assert_eq!(
        store.installation().expect("load installation"),
        Some(installation)
    );
    assert_eq!(
        store.watched_roots().expect("load watched roots"),
        vec![PathBuf::from("/work/alpha"), PathBuf::from("/work/zeta")]
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn installation_selection_is_idempotent_but_cannot_silently_change() {
    let database_path = temporary_database_path("installation-conflict");
    let installation = InstallationRecord::new(
        "install-1",
        EngineProvider::Docker,
        "unix:///var/run/docker.sock",
    );
    let replacement = InstallationRecord::new(
        "install-2",
        EngineProvider::Docker,
        "unix:///Users/example/.docker/run/docker.sock",
    );
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");

    store
        .initialize_installation(&installation)
        .expect("initialize installation");
    store
        .initialize_installation(&installation)
        .expect("replay installation");
    let error = store
        .initialize_installation(&replacement)
        .expect_err("reject replacement installation");

    assert_eq!(
        error.to_string(),
        "Stackctl installation is already initialized as 'install-1'; explicit migration is required"
    );
    assert_eq!(
        store.installation().expect("load installation"),
        Some(installation)
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn project_ownership_survives_store_restart() {
    let database_path = temporary_database_path("restart");
    let record = project_record(
        "/work/bill",
        "bill",
        &[
            "bill-app.stackctl.localhost",
            "bill-mailpit.stackctl.localhost",
        ],
    );

    {
        let mut store = SqliteStateStore::open(&database_path).expect("open state store");
        store
            .replace_project(&record)
            .expect("persist project ownership");
    }

    let store = SqliteStateStore::open(&database_path).expect("reopen state store");

    assert_eq!(store.projects().expect("load projects"), vec![record]);

    drop(store);
    remove_database(&database_path);
}

#[test]
fn conflicting_route_ownership_rolls_back_the_entire_project_write() {
    let database_path = temporary_database_path("collision");
    let first = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let conflicting = project_record(
        "/work/archive/bill",
        "bill",
        &[
            "bill-app.stackctl.localhost",
            "bill-mailpit.stackctl.localhost",
        ],
    );
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .replace_project(&first)
        .expect("persist first project");

    let error = store
        .replace_project(&conflicting)
        .expect_err("route ownership conflict");

    assert_eq!(
        error.to_string(),
        "route 'bill-app.stackctl.localhost' is owned by '/work/bill', not '/work/archive/bill'"
    );
    assert_eq!(store.projects().expect("load projects"), vec![first]);

    drop(store);
    remove_database(&database_path);
}

#[test]
fn dropping_an_uncommitted_transaction_leaves_no_partial_project() {
    let database_path = temporary_database_path("interruption");

    {
        let store = SqliteStateStore::open(&database_path).expect("open state store");
        store
            .connection
            .execute_batch(
                "BEGIN IMMEDIATE;\n\
                 INSERT INTO projects (canonical_path, project_name)\n\
                 VALUES ('/work/partial', 'partial');",
            )
            .expect("write uncommitted project");
    }

    let store = SqliteStateStore::open(&database_path).expect("reopen state store");

    assert!(store.projects().expect("load projects").is_empty());

    drop(store);
    remove_database(&database_path);
}

#[test]
fn complete_resource_ownership_survives_store_restart() {
    let database_path = temporary_database_path("resources");
    let resource = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "container-1".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "shared_service".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:desired-v1".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });

    {
        let mut store = SqliteStateStore::open(&database_path).expect("open state store");
        store
            .upsert_resources(std::slice::from_ref(&resource))
            .expect("persist resource ownership");
    }

    let store = SqliteStateStore::open(&database_path).expect("reopen state store");

    assert_eq!(store.resources().expect("load resources"), vec![resource]);

    drop(store);
    remove_database(&database_path);
}

#[test]
fn retiring_resources_requires_an_exact_orphaned_snapshot_and_is_atomic() {
    let database_path = temporary_database_path("retire-resources");
    let orphaned = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "container-orphaned".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "project_application".to_owned(),
        compatibility_fingerprint: "sha256:runtime".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:desired-v1".to_owned(),
        retention: ResourceRetention::Disposable,
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(10_000),
    })
    .with_scope_id("app");
    let active = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "container-active".to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "project_application".to_owned(),
        compatibility_fingerprint: "sha256:runtime".to_owned(),
        project_id: Some("shop".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:desired-v1".to_owned(),
        retention: ResourceRetention::Disposable,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
    .with_scope_id("app");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .upsert_resources(&[orphaned.clone(), active.clone()])
        .expect("persist resources");

    let error = store
        .retire_resources(&[orphaned.clone(), active.clone()])
        .expect_err("active resources must block the complete retirement batch");

    assert!(error.to_string().contains("container-active"));
    assert_eq!(
        store.resources().expect("unchanged resources"),
        vec![active, orphaned.clone()]
    );

    let drifted = ResourceRecord::new(ResourceRecordOptions {
        resource_id: orphaned.resource_id().to_owned(),
        installation_id: orphaned.installation_id().to_owned(),
        kind: orphaned.kind().to_owned(),
        compatibility_fingerprint: orphaned.compatibility_fingerprint().to_owned(),
        project_id: orphaned.project_id().map(str::to_owned),
        schema_version: orphaned.schema_version(),
        desired_revision: "sha256:changed".to_owned(),
        retention: orphaned.retention(),
        lifecycle: orphaned.lifecycle(),
        orphaned_at_unix_seconds: orphaned.orphaned_at_unix_seconds(),
    })
    .with_scope_id("app");
    let error = store
        .retire_resources(&[drifted])
        .expect_err("changed snapshot must not retire durable ownership");

    assert!(error.to_string().contains("differs from durable ownership"));

    store
        .retire_resources(std::slice::from_ref(&orphaned))
        .expect("retire exact orphaned resource");

    assert_eq!(
        store
            .resources()
            .expect("only active resource remains")
            .len(),
        1
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn retiring_logical_resource_is_exact_idempotent_and_cleans_last_environment() {
    let database_path = temporary_database_path("retire-logical-resource");
    let project = project_record("/work/bill", "bill", &[]);
    let logical = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "stackctl_bill_database".to_owned(),
        shared_resource_id: "postgres-17".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/primary".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "stackctl_bill".to_owned(),
        secret: "runtime-only-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:environment".to_owned(),
        values: BTreeMap::from([("DB_PASSWORD".to_owned(), "runtime-only-secret".to_owned())]),
        lifecycle: EnvironmentLifecycle::Active,
    });
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store.replace_project(&project).expect("register project");
    store
        .record_logical_environment(std::slice::from_ref(&logical), &environment)
        .expect("persist logical environment");
    store
        .insert_credential_if_absent(&credential)
        .expect("persist credential");
    store
        .orphan_project(project.canonical_path(), 10_000)
        .expect("orphan project");
    let orphaned = store.logical_resources().expect("orphaned logical")[0].clone();
    let disabled = store.credentials().expect("disabled credential")[0].clone();
    let drifted = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: orphaned.logical_resource_id().to_owned(),
        shared_resource_id: orphaned.shared_resource_id().to_owned(),
        project_id: orphaned.project_id().to_owned(),
        service_id: orphaned.service_id().to_owned(),
        kind: orphaned.kind().to_owned(),
        compatibility_fingerprint: orphaned.compatibility_fingerprint().to_owned(),
        desired_revision: "sha256:drifted".to_owned(),
        lifecycle: orphaned.lifecycle(),
        orphaned_at_unix_seconds: orphaned.orphaned_at_unix_seconds(),
    });

    let error = store
        .retire_logical_resource(&drifted, &disabled)
        .expect_err("drifted snapshot must fail");

    assert!(error.to_string().contains("differs from durable ownership"));
    assert_eq!(
        store.logical_resources().expect("logical retained"),
        vec![orphaned.clone()]
    );
    assert_eq!(
        store.credentials().expect("credential retained"),
        vec![disabled.clone()]
    );
    assert_eq!(
        store
            .managed_environments()
            .expect("environment retained")
            .len(),
        1
    );

    store
        .retire_logical_resource(&orphaned, &disabled)
        .expect("retire exact logical state");
    store
        .retire_logical_resource(&orphaned, &disabled)
        .expect("idempotent retirement replay");

    assert!(
        store
            .logical_resources()
            .expect("logical retired")
            .is_empty()
    );
    assert!(store.credentials().expect("credential retired").is_empty());
    assert!(
        store
            .managed_environments()
            .expect("environment retired")
            .is_empty()
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn logical_resources_persist_and_reference_count_only_active_consumers() {
    let database_path = temporary_database_path("logical-resources");
    let bill = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let shop = project_record("/work/shop", "shop", &["shop-app.stackctl.localhost"]);
    let bill_database = logical_resource_record("bill/database", "bill", "database");
    let shop_database = logical_resource_record("shop/database", "shop", "database");
    let retained = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "old/database".to_owned(),
        shared_resource_id: "postgres-shared-17".to_owned(),
        project_id: "old".to_owned(),
        service_id: "database".to_owned(),
        kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        desired_revision: "sha256:desired-v1".to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(10_000),
    });
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .replace_projects(&[bill, shop])
        .expect("persist projects");
    store
        .upsert_logical_resources(&[
            bill_database.clone(),
            shop_database.clone(),
            retained.clone(),
        ])
        .expect("persist logical resources");

    assert_eq!(
        store
            .active_logical_reference_count("postgres-shared-17")
            .expect("active references"),
        2
    );
    assert_eq!(
        store.logical_resources().expect("logical resources"),
        vec![bill_database, retained, shop_database]
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn recoverable_open_snapshots_valid_state_and_preserves_backups_on_corruption() {
    let database_path = temporary_database_path("recoverable-open");
    let backup_directory = database_path.with_extension("backups");
    let project = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store.replace_project(&project).expect("persist project");
    drop(store);

    for created_at in 40_000..40_004 {
        drop(
            SqliteStateStore::open_with_backups(&database_path, &backup_directory, created_at)
                .expect("open with state backup"),
        );
    }
    drop(
        SqliteStateStore::open_with_backups(&database_path, &backup_directory, 40_003)
            .expect("reuse same-second recovery point"),
    );

    let mut backups = std::fs::read_dir(&backup_directory)
        .expect("read state backups")
        .map(|entry| entry.expect("backup entry").path())
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("sqlite3"))
        .collect::<Vec<_>>();
    backups.sort();
    assert_eq!(backups.len(), 3);
    assert!(
        backups[0]
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.contains("00000000000000040001"))
    );
    for backup in &backups {
        let connection = rusqlite::Connection::open(backup).expect("open state backup");
        let project_name = connection
            .query_row("SELECT project_name FROM projects", [], |row| {
                row.get::<_, String>(0)
            })
            .expect("read backed-up project");
        assert_eq!(project_name, "bill");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        assert_eq!(
            std::fs::metadata(&backup_directory)
                .expect("backup directory metadata")
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert!(backups.iter().all(|backup| {
            std::fs::metadata(backup)
                .expect("backup metadata")
                .permissions()
                .mode()
                & 0o777
                == 0o600
        }));
    }

    std::fs::write(&database_path, b"not a sqlite database").expect("corrupt primary state");
    let error = match SqliteStateStore::open_with_backups(&database_path, &backup_directory, 50_000)
    {
        Ok(_) => panic!("corrupt state must fail before backup or migration"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("integrity check failed"));
    assert_eq!(
        std::fs::read_dir(&backup_directory)
            .expect("retained state backups")
            .count(),
        3
    );

    std::fs::remove_dir_all(&backup_directory).expect("remove state backups");
    remove_database(&database_path);
}

#[test]
fn orphaning_a_project_releases_its_active_shared_service_reference() {
    let database_path = temporary_database_path("logical-resource-orphan");
    let bill = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let logical = logical_resource_record("bill/database", "bill", "database");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store.replace_project(&bill).expect("persist project");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("persist logical resource");

    store
        .orphan_project(Path::new("/work/bill"), 12_345)
        .expect("orphan project");

    let retained = store
        .logical_resources()
        .expect("retained logical resources")
        .pop()
        .expect("logical resource");
    assert_eq!(retained.lifecycle(), ResourceLifecycle::Orphaned);
    assert_eq!(retained.orphaned_at_unix_seconds(), Some(12_345));
    assert_eq!(
        store
            .active_logical_reference_count("postgres-shared-17")
            .expect("released references"),
        0
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn logical_resource_reconciliation_cannot_reassign_or_implicitly_adopt_tenants() {
    let database_path = temporary_database_path("logical-resource-ownership");
    let project = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let logical = logical_resource_record("bill/database", "bill", "database");
    let forged = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: logical.logical_resource_id().to_owned(),
        shared_resource_id: "foreign-postgres".to_owned(),
        project_id: logical.project_id().to_owned(),
        service_id: logical.service_id().to_owned(),
        kind: logical.kind().to_owned(),
        compatibility_fingerprint: logical.compatibility_fingerprint().to_owned(),
        desired_revision: logical.desired_revision().to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store.replace_project(&project).expect("persist project");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("persist logical resource");

    let error = store
        .upsert_logical_resources(std::slice::from_ref(&forged))
        .expect_err("logical ownership drift");
    assert_eq!(
        error.to_string(),
        "logical resource 'bill/database' has immutable ownership metadata that differs from durable state; explicit adoption or migration is required"
    );

    store
        .orphan_project(project.canonical_path(), 12_345)
        .expect("orphan project");
    let error = store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect_err("implicit logical adoption");
    assert_eq!(
        error.to_string(),
        "project 'bill' has disabled managed state; explicit adoption is required before reactivation"
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn logical_ownership_and_managed_environment_publish_atomically() {
    let database_path = temporary_database_path("logical-environment-transaction");
    let project = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let logical = logical_resource_record("bill/database", "bill", "database");
    let environment = managed_environment(BTreeMap::from([(
        "DB_DATABASE".to_owned(),
        "stackctl_bill_database".to_owned(),
    )]));
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store.replace_project(&project).expect("persist project");

    store
        .record_logical_environment(std::slice::from_ref(&logical), &environment)
        .expect("publish logical environment");

    assert_eq!(
        store.logical_resources().expect("logical resources"),
        vec![logical.clone()]
    );
    assert_eq!(
        store.managed_environments().expect("environments"),
        vec![environment.clone()]
    );

    let conflicting = LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: logical.logical_resource_id().to_owned(),
        shared_resource_id: "foreign-postgres".to_owned(),
        project_id: logical.project_id().to_owned(),
        service_id: logical.service_id().to_owned(),
        kind: logical.kind().to_owned(),
        compatibility_fingerprint: logical.compatibility_fingerprint().to_owned(),
        desired_revision: "sha256:changed".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let changed_environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:changed".to_owned(),
        values: BTreeMap::from([("DB_DATABASE".to_owned(), "changed".to_owned())]),
        lifecycle: EnvironmentLifecycle::Active,
    });

    store
        .record_logical_environment(&[conflicting], &changed_environment)
        .expect_err("ownership conflict rolls back environment");

    assert_eq!(
        store.managed_environments().expect("retained environment"),
        vec![environment]
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn logical_environment_reconciliation_orphans_omitted_services_atomically() {
    let database_path = temporary_database_path("logical-environment-removal");
    let project = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let database = logical_resource_record("bill/database", "bill", "database");
    let cache = logical_resource_record("bill/cache", "bill", "cache");
    let database_credential = credential_record("database-secret");
    let cache_credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/cache/primary".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "cache".to_owned(),
        username: "stackctl_bill".to_owned(),
        secret: "cache-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    let initial_environment = managed_environment(BTreeMap::from([
        ("DB_PASSWORD".to_owned(), "database-secret".to_owned()),
        ("REDIS_PASSWORD".to_owned(), "cache-secret".to_owned()),
    ]));
    let current_environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:environment-v2".to_owned(),
        values: BTreeMap::from([("DB_PASSWORD".to_owned(), "database-secret".to_owned())]),
        lifecycle: EnvironmentLifecycle::Active,
    });
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store.replace_project(&project).expect("persist project");
    store
        .insert_credential_if_absent(&database_credential)
        .expect("persist database credential");
    store
        .insert_credential_if_absent(&cache_credential)
        .expect("persist cache credential");
    store
        .record_logical_environment(&[database.clone(), cache.clone()], &initial_environment)
        .expect("publish initial logical environment");

    store
        .reconcile_logical_environment(
            std::slice::from_ref(&database),
            &current_environment,
            12_345,
        )
        .expect("replace logical environment");

    let logical = store.logical_resources().expect("logical resources");
    assert_eq!(logical[0].logical_resource_id(), "bill/cache");
    assert_eq!(logical[0].lifecycle(), ResourceLifecycle::Orphaned);
    assert_eq!(logical[0].orphaned_at_unix_seconds(), Some(12_345));
    assert_eq!(logical[1], database);
    let credentials = store.credentials().expect("credentials");
    assert_eq!(credentials[0].credential_id(), "bill/cache/primary");
    assert_eq!(credentials[0].lifecycle(), CredentialLifecycle::Disabled);
    assert_eq!(credentials[1], database_credential);
    assert_eq!(
        store.managed_environments().expect("managed environment"),
        vec![current_environment.clone()]
    );
    assert_eq!(
        store
            .active_logical_reference_count("postgres-shared-17")
            .expect("remaining database reference"),
        1
    );
    let empty_environment = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:environment-v3".to_owned(),
        values: BTreeMap::new(),
        lifecycle: EnvironmentLifecycle::Active,
    });

    store
        .reconcile_logical_environment(&[], &empty_environment, 23_456)
        .expect("remove final logical service");

    assert_eq!(
        store
            .active_logical_reference_count("postgres-shared-17")
            .expect("released shared references"),
        0
    );
    assert_eq!(
        store.credentials().expect("disabled credentials")[1].lifecycle(),
        CredentialLifecycle::Disabled
    );
    assert_eq!(
        store.managed_environments().expect("empty environment"),
        vec![empty_environment]
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn resource_upsert_rejects_immutable_ownership_drift_atomically() {
    let database_path = temporary_database_path("resource-ownership-conflict");
    let first = resource_record("container-first", "bill", ResourceRetention::Persistent);
    let second = resource_record("container-second", "shop", ResourceRetention::Persistent);
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .upsert_resources(&[first.clone(), second.clone()])
        .expect("persist initial resources");
    let updated_first = ResourceRecord::new(ResourceRecordOptions {
        resource_id: first.resource_id().to_owned(),
        installation_id: first.installation_id().to_owned(),
        kind: first.kind().to_owned(),
        compatibility_fingerprint: first.compatibility_fingerprint().to_owned(),
        project_id: first.project_id().map(str::to_owned),
        schema_version: first.schema_version(),
        desired_revision: "sha256:desired-v2".to_owned(),
        retention: first.retention(),
        lifecycle: ResourceLifecycle::Retained,
        orphaned_at_unix_seconds: Some(20_000),
    });
    let forged_second = ResourceRecord::new(ResourceRecordOptions {
        resource_id: second.resource_id().to_owned(),
        installation_id: "foreign-installation".to_owned(),
        kind: second.kind().to_owned(),
        compatibility_fingerprint: second.compatibility_fingerprint().to_owned(),
        project_id: second.project_id().map(str::to_owned),
        schema_version: second.schema_version(),
        desired_revision: "sha256:forged".to_owned(),
        retention: second.retention(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });

    let error = store
        .upsert_resources(&[updated_first, forged_second])
        .expect_err("ownership drift");

    assert_eq!(
        error.to_string(),
        "resource 'container-second' has immutable ownership metadata that differs from durable state; explicit adoption or migration is required"
    );
    assert_eq!(
        store.resources().expect("unchanged resources"),
        vec![first, second]
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn generic_resource_upsert_cannot_reactivate_orphaned_data() {
    let database_path = temporary_database_path("resource-adoption-required");
    let active = resource_record("container-bill", "bill", ResourceRetention::Persistent);
    let orphaned = ResourceRecord::new(ResourceRecordOptions {
        resource_id: active.resource_id().to_owned(),
        installation_id: active.installation_id().to_owned(),
        kind: active.kind().to_owned(),
        compatibility_fingerprint: active.compatibility_fingerprint().to_owned(),
        project_id: active.project_id().map(str::to_owned),
        schema_version: active.schema_version(),
        desired_revision: active.desired_revision().to_owned(),
        retention: active.retention(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(20_000),
    });
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .upsert_resources(std::slice::from_ref(&orphaned))
        .expect("persist orphan");

    let error = store
        .upsert_resources(std::slice::from_ref(&active))
        .expect_err("implicit adoption");

    assert_eq!(
        error.to_string(),
        "resource 'container-bill' is orphaned or retained; explicit adoption is required before reactivation"
    );
    assert_eq!(store.resources().expect("retained orphan"), vec![orphaned]);

    drop(store);
    remove_database(&database_path);
}

#[test]
fn physical_reconciliation_retires_replaced_backend_identities() {
    let database_path = temporary_database_path("resource-replacement");
    let old = resource_record("postgres-old", "bill", ResourceRetention::Persistent);
    let current = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "postgres-current".to_owned(),
        installation_id: old.installation_id().to_owned(),
        kind: old.kind().to_owned(),
        compatibility_fingerprint: old.compatibility_fingerprint().to_owned(),
        project_id: old.project_id().map(str::to_owned),
        schema_version: old.schema_version(),
        desired_revision: "sha256:desired-v2".to_owned(),
        retention: old.retention(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .upsert_resources(std::slice::from_ref(&old))
        .expect("persist old backend identity");

    store
        .reconcile_resources(std::slice::from_ref(&current), 12_345)
        .expect("reconcile replacement");

    let resources = store.resources().expect("reconciled resources");
    assert_eq!(resources.len(), 2);
    assert_eq!(resources[0], current);
    assert_eq!(resources[1].resource_id(), "postgres-old");
    assert_eq!(resources[1].lifecycle(), ResourceLifecycle::Retained);
    assert_eq!(resources[1].orphaned_at_unix_seconds(), Some(12_345));

    drop(store);
    remove_database(&database_path);
}

#[test]
fn project_resource_reconciliation_retires_the_same_scope_across_runtime_changes() {
    let database_path = temporary_database_path("project-resource-scope-replacement");
    let old = resource_record("application-old", "bill", ResourceRetention::Disposable)
        .with_scope_id("app");
    let current = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "application-current".to_owned(),
        installation_id: old.installation_id().to_owned(),
        kind: old.kind().to_owned(),
        compatibility_fingerprint: "sha256:runtime-v2".to_owned(),
        project_id: old.project_id().map(str::to_owned),
        schema_version: old.schema_version(),
        desired_revision: "sha256:desired-v2".to_owned(),
        retention: old.retention(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
    .with_scope_id("app");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .upsert_resources(std::slice::from_ref(&old))
        .expect("persist old application");

    store
        .reconcile_resources(std::slice::from_ref(&current), 12_345)
        .expect("reconcile changed application runtime");

    let resources = store.resources().expect("reconciled resources");
    assert_eq!(resources.len(), 2);
    assert_eq!(resources[0], current);
    assert_eq!(resources[1].resource_id(), "application-old");
    assert_eq!(resources[1].lifecycle(), ResourceLifecycle::Retained);
    assert_eq!(resources[1].orphaned_at_unix_seconds(), Some(12_345));

    drop(store);
    remove_database(&database_path);
}

#[test]
fn project_resource_reconciliation_keeps_distinct_service_scopes_active() {
    let database_path = temporary_database_path("project-resource-distinct-scopes");
    let application =
        resource_record("application", "bill", ResourceRetention::Disposable).with_scope_id("app");
    let worker = ResourceRecord::new(ResourceRecordOptions {
        resource_id: "worker".to_owned(),
        installation_id: application.installation_id().to_owned(),
        kind: application.kind().to_owned(),
        compatibility_fingerprint: application.compatibility_fingerprint().to_owned(),
        project_id: application.project_id().map(str::to_owned),
        schema_version: application.schema_version(),
        desired_revision: application.desired_revision().to_owned(),
        retention: application.retention(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
    .with_scope_id("worker");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .reconcile_resources(&[application.clone(), worker.clone()], 12_345)
        .expect("reconcile project services");

    assert_eq!(
        store.resources().expect("active project services"),
        vec![application, worker]
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn unregistering_a_project_atomically_orphans_only_its_resources() {
    let database_path = temporary_database_path("project-orphan");
    let bill = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let shop = project_record("/work/shop", "shop", &["shop-app.stackctl.localhost"]);
    let bill_resource = resource_record("container-bill", "bill", ResourceRetention::Persistent);
    let shop_resource = resource_record("container-shop", "shop", ResourceRetention::Disposable);
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .replace_projects(&[bill, shop.clone()])
        .expect("persist projects");
    store
        .upsert_resources(&[bill_resource, shop_resource.clone()])
        .expect("persist resources");

    store
        .orphan_project(Path::new("/work/bill"), 12_345)
        .expect("orphan missing project");

    assert_eq!(store.projects().expect("load projects"), vec![shop]);
    assert_eq!(
        store.resources().expect("load resources"),
        vec![
            ResourceRecord::new(ResourceRecordOptions {
                resource_id: "container-bill".to_owned(),
                installation_id: "install-1".to_owned(),
                kind: "application".to_owned(),
                compatibility_fingerprint: "sha256:runtime".to_owned(),
                project_id: Some("bill".to_owned()),
                schema_version: 8,
                desired_revision: "sha256:desired-v1".to_owned(),
                retention: ResourceRetention::Persistent,
                lifecycle: ResourceLifecycle::Orphaned,
                orphaned_at_unix_seconds: Some(12_345),
            }),
            shop_resource,
        ]
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn complete_registry_reconciliation_orphans_projects_missing_from_the_scan() {
    let database_path = temporary_database_path("complete-registry-reconcile");
    let bill = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let shop = project_record("/work/shop", "shop", &["shop-app.stackctl.localhost"]);
    let bill_resource = resource_record("container-bill", "bill", ResourceRetention::Persistent);
    let bill_logical = logical_resource_record("bill/database", "bill", "database");
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .replace_projects(&[bill, shop.clone()])
        .expect("persist initial registry");
    store
        .upsert_resources(std::slice::from_ref(&bill_resource))
        .expect("persist resource");
    store
        .upsert_logical_resources(std::slice::from_ref(&bill_logical))
        .expect("persist logical resource");
    store
        .insert_credential_if_absent(&credential_record("secret-first"))
        .expect("persist credential");
    store
        .replace_managed_environment(&managed_environment(BTreeMap::new()))
        .expect("persist environment");

    store
        .reconcile_project_registry(std::slice::from_ref(&shop), 12_345)
        .expect("reconcile complete registry");

    assert_eq!(store.projects().expect("remaining projects"), vec![shop]);
    assert_eq!(
        store.resources().expect("orphaned resources")[0].lifecycle(),
        ResourceLifecycle::Orphaned
    );
    assert_eq!(
        store
            .logical_resources()
            .expect("orphaned logical resources")[0]
            .lifecycle(),
        ResourceLifecycle::Orphaned
    );
    assert_eq!(
        store.credentials().expect("disabled credentials")[0].lifecycle(),
        CredentialLifecycle::Disabled
    );
    assert_eq!(
        store.managed_environments().expect("disabled environment")[0].lifecycle(),
        EnvironmentLifecycle::Disabled
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn credentials_remain_stable_redacted_and_durable() {
    let database_path = temporary_database_path("credentials");
    let credential = credential_record("secret-first");

    {
        let mut store = SqliteStateStore::open(&database_path).expect("open state store");
        let inserted = store
            .insert_credential_if_absent(&credential)
            .expect("insert credential");
        let replay = store
            .insert_credential_if_absent(&credential_record("secret-replacement"))
            .expect("reconcile credential");

        assert_eq!(inserted, credential);
        assert_eq!(replay, credential);
        assert!(!format!("{replay:?}").contains("secret-first"));
        assert!(format!("{replay:?}").contains("[REDACTED]"));
    }

    let store = SqliteStateStore::open(&database_path).expect("reopen state store");

    assert_eq!(
        store.credentials().expect("load credentials"),
        vec![credential]
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn unregistering_a_project_disables_credentials_without_deleting_them() {
    let database_path = temporary_database_path("credential-orphan");
    let project = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store.replace_project(&project).expect("persist project");
    store
        .insert_credential_if_absent(&credential_record("secret-first"))
        .expect("insert credential");

    store
        .orphan_project(Path::new("/work/bill"), 12_345)
        .expect("orphan project");

    let credential = store
        .credentials()
        .expect("load credentials")
        .pop()
        .expect("retained credential");
    assert_eq!(credential.lifecycle(), CredentialLifecycle::Disabled);
    assert_eq!(credential.secret(), "secret-first");

    drop(store);
    remove_database(&database_path);
}

#[test]
fn managed_environment_replaces_completely_and_survives_restart() {
    let database_path = temporary_database_path("managed-environment");
    let initial = managed_environment(BTreeMap::from([
        (
            "DB_DATABASE".to_owned(),
            "stackctl_bill_database".to_owned(),
        ),
        ("DB_PASSWORD".to_owned(), "project-secret".to_owned()),
    ]));
    let replacement = ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:environment-v2".to_owned(),
        values: BTreeMap::from([(
            "DB_DATABASE".to_owned(),
            "stackctl_bill_database".to_owned(),
        )]),
        lifecycle: EnvironmentLifecycle::Active,
    });

    {
        let mut store = SqliteStateStore::open(&database_path).expect("open state store");
        store
            .replace_managed_environment(&initial)
            .expect("persist environment");
        store
            .replace_managed_environment(&replacement)
            .expect("replace environment");
        assert!(!format!("{initial:?}").contains("project-secret"));
        assert!(format!("{initial:?}").contains("DB_PASSWORD"));
    }

    let store = SqliteStateStore::open(&database_path).expect("reopen state store");

    assert_eq!(
        store
            .managed_environments()
            .expect("load managed environments"),
        vec![replacement]
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn unregistering_a_project_disables_its_managed_environment() {
    let database_path = temporary_database_path("environment-orphan");
    let project = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store.replace_project(&project).expect("persist project");
    store
        .replace_managed_environment(&managed_environment(BTreeMap::from([(
            "DB_PASSWORD".to_owned(),
            "project-secret".to_owned(),
        )])))
        .expect("persist environment");

    store
        .orphan_project(Path::new("/work/bill"), 12_345)
        .expect("orphan project");

    assert_eq!(
        store
            .managed_environments()
            .expect("load managed environment")[0]
            .lifecycle(),
        EnvironmentLifecycle::Disabled
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn replacing_managed_environment_cannot_implicitly_reactivate_an_orphan() {
    let database_path = temporary_database_path("environment-adoption-required");
    let project = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let environment = managed_environment(BTreeMap::from([(
        "DB_PASSWORD".to_owned(),
        "project-secret".to_owned(),
    )]));
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store.replace_project(&project).expect("persist project");
    store
        .replace_managed_environment(&environment)
        .expect("persist environment");
    store
        .orphan_project(Path::new("/work/bill"), 12_345)
        .expect("orphan project");

    let error = store
        .replace_managed_environment(&environment)
        .expect_err("implicit environment adoption");

    assert_eq!(
        error.to_string(),
        "project 'bill' has disabled managed state; explicit adoption is required before reactivation"
    );
    assert_eq!(
        store.managed_environments().expect("retained environment")[0].lifecycle(),
        EnvironmentLifecycle::Disabled
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn explicit_project_adoption_reactivates_resources_credentials_and_environment_atomically() {
    let database_path = temporary_database_path("complete-project-adoption");
    let project = project_record("/work/bill", "bill", &["bill-app.stackctl.localhost"]);
    let resource = resource_record("container-bill", "bill", ResourceRetention::Persistent);
    let logical = logical_resource_record("bill/database", "bill", "database");
    let credential = credential_record("secret-first");
    let environment = managed_environment(BTreeMap::from([(
        "DB_PASSWORD".to_owned(),
        "secret-first".to_owned(),
    )]));
    let mut store = SqliteStateStore::open(&database_path).expect("open state store");
    store
        .replace_project(&project)
        .expect("persist original project");
    store
        .upsert_resources(std::slice::from_ref(&resource))
        .expect("persist resource");
    store
        .upsert_logical_resources(std::slice::from_ref(&logical))
        .expect("persist logical resource");
    store
        .insert_credential_if_absent(&credential)
        .expect("persist credential");
    store
        .replace_managed_environment(&environment)
        .expect("persist environment");
    store
        .orphan_project(project.canonical_path(), 12_345)
        .expect("orphan project");
    store
        .replace_project(&project)
        .expect("register adoption target");
    let incompatible_resource = ResourceRecord::new(ResourceRecordOptions {
        resource_id: resource.resource_id().to_owned(),
        installation_id: resource.installation_id().to_owned(),
        kind: resource.kind().to_owned(),
        compatibility_fingerprint: "sha256:different-runtime".to_owned(),
        project_id: resource.project_id().map(str::to_owned),
        schema_version: resource.schema_version(),
        desired_revision: resource.desired_revision().to_owned(),
        retention: resource.retention(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    });
    let incompatible = ProjectAdoptionPlan::new(ProjectAdoptionPlanOptions {
        canonical_path: project.canonical_path().to_path_buf(),
        project_id: "bill".to_owned(),
        resources: vec![incompatible_resource],
        logical_resources: vec![logical.clone()],
        credential_ids: vec![credential.credential_id().to_owned()],
        environment_revision: environment.revision().to_owned(),
    })
    .expect("incompatible adoption plan");
    let error = store
        .adopt_project(&incompatible)
        .expect_err("incompatible resource adoption");
    assert_eq!(
        error.to_string(),
        "resource 'container-bill' has immutable ownership metadata that differs from durable state; explicit adoption or migration is required"
    );
    let incomplete = ProjectAdoptionPlan::new(ProjectAdoptionPlanOptions {
        canonical_path: project.canonical_path().to_path_buf(),
        project_id: "bill".to_owned(),
        resources: vec![resource.clone()],
        logical_resources: vec![logical.clone()],
        credential_ids: vec!["bill/database/missing".to_owned()],
        environment_revision: environment.revision().to_owned(),
    })
    .expect("incomplete adoption plan");
    let error = store
        .adopt_project(&incomplete)
        .expect_err("missing retained credential");
    assert_eq!(
        error.to_string(),
        "project 'bill' adoption does not match retained state: credential 'bill/database/missing' is missing or owned by another project"
    );
    assert_eq!(
        store.resources().expect("still orphaned")[0].lifecycle(),
        ResourceLifecycle::Orphaned
    );
    assert_eq!(
        store.managed_environments().expect("still disabled")[0].lifecycle(),
        EnvironmentLifecycle::Disabled
    );
    let adoption = ProjectAdoptionPlan::new(ProjectAdoptionPlanOptions {
        canonical_path: project.canonical_path().to_path_buf(),
        project_id: "bill".to_owned(),
        resources: vec![resource.clone()],
        logical_resources: vec![logical.clone()],
        credential_ids: vec![credential.credential_id().to_owned()],
        environment_revision: environment.revision().to_owned(),
    })
    .expect("project adoption plan");

    store.adopt_project(&adoption).expect("adopt project state");

    assert_eq!(store.resources().expect("active resources"), vec![resource]);
    assert_eq!(
        store.logical_resources().expect("active logical resources"),
        vec![logical]
    );
    assert_eq!(
        store.credentials().expect("active credentials")[0].lifecycle(),
        CredentialLifecycle::Active
    );
    assert_eq!(
        store.managed_environments().expect("active environment")[0].lifecycle(),
        EnvironmentLifecycle::Active
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn version_one_state_migrates_without_losing_project_ownership() {
    let database_path = temporary_database_path("v1-migration");
    let connection = rusqlite::Connection::open(&database_path).expect("open legacy state");
    connection
        .execute_batch(
            "CREATE TABLE projects (\n\
                 canonical_path TEXT PRIMARY KEY NOT NULL,\n\
                 project_name TEXT NOT NULL\n\
             ) STRICT;\n\
             CREATE TABLE route_claims (\n\
                 domain TEXT PRIMARY KEY NOT NULL,\n\
                 canonical_path TEXT NOT NULL\n\
                     REFERENCES projects(canonical_path) ON DELETE CASCADE\n\
             ) STRICT;\n\
             INSERT INTO projects VALUES ('/work/bill', 'bill');\n\
             INSERT INTO route_claims VALUES\n\
                 ('bill-app.stackctl.localhost', '/work/bill');\n\
             PRAGMA user_version = 1;",
        )
        .expect("seed legacy schema");
    drop(connection);

    let store = SqliteStateStore::open(&database_path).expect("migrate state store");

    assert_eq!(store.schema_version().expect("schema version"), 18);
    assert_eq!(
        store.projects().expect("preserved projects"),
        vec![project_record(
            "/work/bill",
            "bill",
            &["bill-app.stackctl.localhost"]
        )]
    );
    assert!(store.resources().expect("new resource table").is_empty());

    drop(store);
    remove_database(&database_path);
}

#[test]
fn version_five_credentials_migrate_without_losing_ownership_or_secrets() {
    let database_path = temporary_database_path("v5-credential-migration");
    let connection = rusqlite::Connection::open(&database_path).expect("open legacy state");
    connection
        .execute_batch(
            "CREATE TABLE resources (\n\
                 resource_id TEXT PRIMARY KEY NOT NULL,\n\
                 installation_id TEXT NOT NULL,\n\
                 kind TEXT NOT NULL,\n\
                 compatibility_fingerprint TEXT NOT NULL,\n\
                 project_id TEXT,\n\
                 resource_schema_version INTEGER NOT NULL\n\
                     CHECK(resource_schema_version > 0),\n\
                 desired_revision TEXT NOT NULL,\n\
                 retention TEXT NOT NULL\n\
                     CHECK(retention IN ('persistent', 'disposable', 'build_cache')),\n\
                 lifecycle TEXT NOT NULL\n\
                     CHECK(lifecycle IN ('active', 'orphaned', 'retained')),\n\
                 orphaned_at_unix_seconds INTEGER\n\
             ) STRICT;\n\
             CREATE TABLE credentials (\n\
                 credential_id TEXT PRIMARY KEY NOT NULL,\n\
                 project_id TEXT NOT NULL CHECK(length(project_id) > 0),\n\
                 service_id TEXT NOT NULL CHECK(length(service_id) > 0),\n\
                 username TEXT NOT NULL CHECK(length(username) > 0),\n\
                 secret TEXT NOT NULL CHECK(length(secret) > 0),\n\
                 lifecycle TEXT NOT NULL CHECK(lifecycle IN ('active', 'disabled'))\n\
             ) STRICT;\n\
             CREATE INDEX credentials_project_idx ON credentials(project_id);\n\
             INSERT INTO credentials VALUES (\n\
                 'bill/database/primary', 'bill', 'database',\n\
                 'stackctl_bill', 'secret-first', 'active'\n\
             );\n\
             PRAGMA user_version = 5;",
        )
        .expect("seed version-five credentials");
    drop(connection);

    let store = SqliteStateStore::open(&database_path).expect("migrate state store");

    assert_eq!(store.schema_version().expect("schema version"), 18);
    assert_eq!(
        store.credentials().expect("preserved credentials"),
        vec![credential_record("secret-first")]
    );

    drop(store);
    remove_database(&database_path);
}

#[test]
fn version_nine_resources_gain_an_empty_scope_without_losing_ownership() {
    let database_path = temporary_database_path("v9-resource-scope-migration");
    let connection = rusqlite::Connection::open(&database_path).expect("open legacy state");
    connection
        .execute_batch(
            "CREATE TABLE resources (\n\
                 resource_id TEXT PRIMARY KEY NOT NULL,\n\
                 installation_id TEXT NOT NULL,\n\
                 kind TEXT NOT NULL,\n\
                 compatibility_fingerprint TEXT NOT NULL,\n\
                 project_id TEXT,\n\
                 resource_schema_version INTEGER NOT NULL\n\
                     CHECK(resource_schema_version > 0),\n\
                 desired_revision TEXT NOT NULL,\n\
                 retention TEXT NOT NULL\n\
                     CHECK(retention IN ('persistent', 'disposable', 'build_cache')),\n\
                 lifecycle TEXT NOT NULL\n\
                     CHECK(lifecycle IN ('active', 'orphaned', 'retained')),\n\
                 orphaned_at_unix_seconds INTEGER\n\
             ) STRICT;\n\
             INSERT INTO resources VALUES (\n\
                 'shared-postgres', 'install-1', 'shared_service',\n\
                 'sha256:postgres-17', NULL, 8, 'sha256:desired-v1',\n\
                 'persistent', 'active', NULL\n\
             );\n\
             PRAGMA user_version = 9;",
        )
        .expect("seed version-nine resources");
    drop(connection);

    let store = SqliteStateStore::open(&database_path).expect("migrate state store");
    let resources = store.resources().expect("preserved resources");

    assert_eq!(store.schema_version().expect("schema version"), 18);
    assert_eq!(resources.len(), 1);
    assert_eq!(resources[0].resource_id(), "shared-postgres");
    assert_eq!(resources[0].scope_id(), None);
    assert_eq!(
        resources[0].compatibility_fingerprint(),
        "sha256:postgres-17"
    );

    drop(store);
    remove_database(&database_path);
}

fn project_record(path: &str, name: &str, domains: &[&str]) -> ProjectRecord {
    ProjectRecord::new(
        PathBuf::from(path),
        name.to_owned(),
        domains.iter().map(|domain| (*domain).to_owned()).collect(),
    )
}

fn accepted_v7_inventory(seed: &str, accepted_at_unix_seconds: i64) -> AcceptedV7InventoryRecord {
    accepted_v7_inventory_at("bill", "/work/bill", seed, accepted_at_unix_seconds)
}

fn accepted_v7_inventory_at(
    project_id: &str,
    canonical_project_path: &str,
    seed: &str,
    accepted_at_unix_seconds: i64,
) -> AcceptedV7InventoryRecord {
    AcceptedV7InventoryRecord::new(AcceptedV7InventoryRecordOptions {
        project_id: project_id.to_owned(),
        canonical_project_path: PathBuf::from(canonical_project_path),
        source_revision: format!("sha256:{}", seed.repeat(64)),
        inventory_json: format!(
            r#"{{"project_id":"{project_id}","canonical_project_path":"{canonical_project_path}","source_revision":"sha256:{}","blockers":[],"seed":"{seed}"}}"#,
            seed.repeat(64),
        ),
        generated_environment_rollback: None,
        accepted_at_unix_seconds,
    })
    .expect("valid accepted v7 inventory")
}

fn resource_record(
    resource_id: &str,
    project_id: &str,
    retention: ResourceRetention,
) -> ResourceRecord {
    ResourceRecord::new(ResourceRecordOptions {
        resource_id: resource_id.to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "application".to_owned(),
        compatibility_fingerprint: "sha256:runtime".to_owned(),
        project_id: Some(project_id.to_owned()),
        schema_version: 8,
        desired_revision: "sha256:desired-v1".to_owned(),
        retention,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
}

fn logical_resource_record(
    logical_resource_id: &str,
    project_id: &str,
    service_id: &str,
) -> LogicalResourceRecord {
    logical_resource_record_with_lifecycle(
        logical_resource_id,
        project_id,
        service_id,
        ResourceLifecycle::Active,
    )
}

fn logical_resource_record_with_lifecycle(
    logical_resource_id: &str,
    project_id: &str,
    service_id: &str,
    lifecycle: ResourceLifecycle,
) -> LogicalResourceRecord {
    LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: logical_resource_id.to_owned(),
        shared_resource_id: "postgres-shared-17".to_owned(),
        project_id: project_id.to_owned(),
        service_id: service_id.to_owned(),
        kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        desired_revision: "sha256:desired-v1".to_owned(),
        lifecycle,
        orphaned_at_unix_seconds: None,
    })
}

fn credential_record(secret: &str) -> CredentialRecord {
    CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/primary".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "stackctl_bill".to_owned(),
        secret: secret.to_owned(),
        lifecycle: CredentialLifecycle::Active,
    })
}

fn managed_environment(values: BTreeMap<String, String>) -> ManagedEnvironmentRecord {
    ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: "sha256:environment-v1".to_owned(),
        values,
        lifecycle: EnvironmentLifecycle::Active,
    })
}

fn migration_record(phase: MigrationPhase, updated_at_unix_seconds: i64) -> MigrationRecord {
    MigrationRecord::new(migration_options(phase, updated_at_unix_seconds))
        .expect("valid migration record")
}

fn migration_options(
    phase: MigrationPhase,
    updated_at_unix_seconds: i64,
) -> MigrationRecordOptions {
    MigrationRecordOptions {
        migration_id: "migration-bill".to_owned(),
        project_id: "bill".to_owned(),
        source_revision: "sha256:v7".to_owned(),
        target_revision: "sha256:v8".to_owned(),
        source_compatibility_fingerprint: "sha256:postgres-16".to_owned(),
        target_compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        phase,
        backup_reference: (phase >= MigrationPhase::BackupVerified)
            .then(|| "backup:resource-1/40000".to_owned()),
        backup_artifact_sha256: (phase >= MigrationPhase::BackupVerified)
            .then(|| "sha256:backup".to_owned()),
        backup_artifact_size_bytes: (phase >= MigrationPhase::BackupVerified).then_some(1_024),
        target_resource_id: (phase >= MigrationPhase::TargetProvisioned)
            .then(|| "postgres-shared-17".to_owned()),
        rollback_reference: (phase >= MigrationPhase::Cutover)
            .then(|| "retained:v7-resource".to_owned()),
        updated_at_unix_seconds,
    }
}

fn temporary_database_path(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after epoch")
        .as_nanos();

    std::env::temp_dir().join(format!(
        "stackctl-v8-{name}-{}-{unique}.sqlite3",
        std::process::id()
    ))
}

fn remove_database(database_path: &Path) {
    for suffix in ["", "-shm", "-wal"] {
        let path = PathBuf::from(format!("{}{suffix}", database_path.display()));
        if path.exists() {
            std::fs::remove_file(path).expect("remove temporary state database");
        }
    }
}
