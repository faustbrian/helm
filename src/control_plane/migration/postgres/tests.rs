use super::{
    EnginePostgresSourceRetirement, PostgresBackupOptions, PostgresMigrationOperations,
    PostgresMigrationOperationsOptions, PostgresProvisionTargetOptions, PostgresRestoreOptions,
    PostgresSourceRetirement, PostgresSourceRetirementOptions, PostgresVerifyTargetOptions,
    backup_postgres_database, provision_postgres_target, restore_postgres_database,
    verify_postgres_target,
};
use crate::control_plane::engine::{
    CommandExecutionId, CommandExecutor, CommandRequest, CommandSession, CommandStatus,
    ContainerId, ContainerLogStream, EngineFuture, LogChunk, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ObservedContainer, OwnedContainer, ResourceKind,
    RetentionClass, reconstruct_owned_container,
};
use crate::control_plane::migration::{
    MigrationCutoverPlan, MigrationExecutionResult, MigrationFuture, MigrationOperations,
    MigrationRollbackPlan, confirm_migration, execute_migration,
};
use crate::control_plane::retention::{
    BackupResourceIdentity, store_backup_artifact_for_identity, verify_stored_backup_artifact,
};
use crate::control_plane::shared_infrastructure::{CredentialSecret, PostgresLogicalResourcePlan};
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, EnvironmentLifecycle,
    LogicalResourceRecord, LogicalResourceRecordOptions, ManagedEnvironmentRecord,
    ManagedEnvironmentRecordOptions, MigrationPhase, MigrationRecord, MigrationRecordOptions,
    ProjectRecord, ResourceLifecycle, SqliteStateStore, StateStore, retained_migration_source,
};
use futures_util::stream;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, duplex};

#[cfg(unix)]
#[test]
fn postgres_backup_streams_verified_custom_dump_without_host_cli() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("PostgreSQL backup runtime");
    let root = backup_root("success");
    let dump = vec![b'p'; 2 * 1024 * 1024 + 19];
    let executor = RecordingExecutor::new(dump.clone(), 0);
    let logical = logical_resource();
    let credential = credential();
    let container = owned_container();
    let options = PostgresBackupOptions {
        logical_resource: &logical,
        credential: &credential,
        database_name: "stackctl_bill_database",
        installation_id: "install-1",
        created_at_unix_seconds: 45_000,
        backup_root: &root,
        timeout: Duration::from_secs(5),
    };

    let backup = runtime
        .block_on(backup_postgres_database(&executor, &container, &options))
        .expect("PostgreSQL backup");

    assert!(!format!("{options:?}").contains("do-not-log"));
    let expected_request = CommandRequest::new(
        vec![
            "pg_dump".to_owned(),
            "--format=custom".to_owned(),
            "--no-owner".to_owned(),
            "--no-privileges".to_owned(),
            "--username=stackctl_admin".to_owned(),
            "--dbname=stackctl_bill_database".to_owned(),
        ],
        BTreeMap::from([("PGPASSWORD".to_owned(), "do-not-log".to_owned())]),
        None,
    )
    .expect("expected request");
    assert_eq!(
        *executor.request.lock().expect("recorded request"),
        Some(expected_request)
    );
    assert_eq!(
        std::fs::read(Path::new(backup.reference()).join("artifact.bin")).expect("stored dump"),
        dump
    );
    assert_eq!(backup.artifact_size_bytes(), 2 * 1024 * 1024 + 19);
    assert!(!backup.artifact_sha256().is_empty());

    std::fs::remove_dir_all(&root).expect("remove PostgreSQL backup fixture");
}

#[cfg(unix)]
#[test]
fn failed_postgres_dump_never_publishes_partial_recovery_point() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("PostgreSQL backup runtime");
    let root = backup_root("failed");
    let executor = RecordingExecutor::new(b"partial dump".to_vec(), 7);
    let logical = logical_resource();
    let credential = credential();
    let container = owned_container();
    let options = PostgresBackupOptions {
        logical_resource: &logical,
        credential: &credential,
        database_name: "stackctl_bill_database",
        installation_id: "install-1",
        created_at_unix_seconds: 46_000,
        backup_root: &root,
        timeout: Duration::from_secs(5),
    };

    let error = runtime
        .block_on(backup_postgres_database(&executor, &container, &options))
        .expect_err("failed PostgreSQL dump");

    assert_eq!(
        error.to_string(),
        "PostgreSQL backup failed: container 'postgres-source' exited with status 7"
    );
    assert!(!contains_manifest(&root));

    if root.exists() {
        std::fs::remove_dir_all(&root).expect("remove failed backup fixture");
    }
}

#[cfg(unix)]
#[test]
fn postgres_restore_verifies_journaled_backup_before_streaming_to_target() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("PostgreSQL restore runtime");
    let root = backup_root("restore");
    let logical = logical_resource();
    let identity = BackupResourceIdentity::from_logical(&logical, "install-1");
    let dump = b"verified custom dump";
    let stored = store_backup_artifact_for_identity(&identity, dump, 47_000, &root)
        .expect("stored restore fixture");
    let evidence = verify_stored_backup_artifact(&stored, 47_001).expect("backup evidence");
    let checkpoint = restore_checkpoint(
        stored.recovery_point().to_str().expect("backup reference"),
        evidence.artifact_sha256(),
        evidence.artifact_size_bytes(),
    );
    let credential = project_credential();
    let container = owned_container();
    let executor = RecordingExecutor::new(Vec::new(), 0);
    let options = PostgresRestoreOptions {
        checkpoint: &checkpoint,
        source_logical_resource: &logical,
        credential: &credential,
        installation_id: "install-1",
        target_database_name: "stackctl_bill_database_restore",
        target_role_name: "stackctl_bill_database_role",
        verified_at_unix_seconds: 47_001,
        timeout: Duration::from_secs(5),
    };

    runtime
        .block_on(restore_postgres_database(&executor, &container, &options))
        .expect("PostgreSQL restore");

    assert_eq!(*executor.input.lock().expect("restored input"), dump);
    let expected_request = CommandRequest::new(
        vec![
            "pg_restore".to_owned(),
            "--exit-on-error".to_owned(),
            "--single-transaction".to_owned(),
            "--no-owner".to_owned(),
            "--no-privileges".to_owned(),
            "--username=stackctl_bill_database_role".to_owned(),
            "--dbname=stackctl_bill_database_restore".to_owned(),
        ],
        BTreeMap::from([("PGPASSWORD".to_owned(), "project-secret".to_owned())]),
        None,
    )
    .expect("expected restore request");
    assert_eq!(
        *executor.request.lock().expect("restore request"),
        Some(expected_request)
    );

    std::fs::remove_dir_all(&root).expect("remove PostgreSQL restore fixture");
}

#[cfg(unix)]
#[test]
fn postgres_restore_rejects_journal_checksum_mismatch_before_target_command() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("PostgreSQL restore runtime");
    let root = backup_root("restore-mismatch");
    let logical = logical_resource();
    let identity = BackupResourceIdentity::from_logical(&logical, "install-1");
    let stored = store_backup_artifact_for_identity(&identity, b"verified dump", 48_000, &root)
        .expect("stored mismatch fixture");
    let checkpoint = restore_checkpoint(
        stored.recovery_point().to_str().expect("backup reference"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        13,
    );
    let credential = project_credential();
    let container = owned_container();
    let executor = RecordingExecutor::new(Vec::new(), 0);
    let options = PostgresRestoreOptions {
        checkpoint: &checkpoint,
        source_logical_resource: &logical,
        credential: &credential,
        installation_id: "install-1",
        target_database_name: "stackctl_bill_database_restore",
        target_role_name: "stackctl_bill_database_role",
        verified_at_unix_seconds: 48_001,
        timeout: Duration::from_secs(5),
    };

    let error = runtime
        .block_on(restore_postgres_database(&executor, &container, &options))
        .expect_err("mismatched PostgreSQL restore");

    assert_eq!(
        error.to_string(),
        "PostgreSQL restore backup does not match its durable checkpoint"
    );
    assert!(executor.request.lock().expect("restore request").is_none());

    std::fs::remove_dir_all(&root).expect("remove restore mismatch fixture");
}

#[cfg(unix)]
#[test]
fn postgres_restore_rejects_administrator_as_the_target_owner() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("PostgreSQL restore runtime");
    let root = backup_root("restore-owner");
    let logical = logical_resource();
    let identity = BackupResourceIdentity::from_logical(&logical, "install-1");
    let stored = store_backup_artifact_for_identity(&identity, b"verified dump", 49_000, &root)
        .expect("stored owner fixture");
    let evidence = verify_stored_backup_artifact(&stored, 49_001).expect("backup evidence");
    let checkpoint = restore_checkpoint(
        stored.recovery_point().to_str().expect("backup reference"),
        evidence.artifact_sha256(),
        evidence.artifact_size_bytes(),
    );
    let administrator = credential();
    let container = owned_container();
    let executor = RecordingExecutor::new(Vec::new(), 0);
    let options = PostgresRestoreOptions {
        checkpoint: &checkpoint,
        source_logical_resource: &logical,
        credential: &administrator,
        installation_id: "install-1",
        target_database_name: "stackctl_bill_database_restore",
        target_role_name: "stackctl_bill_database_role",
        verified_at_unix_seconds: 49_001,
        timeout: Duration::from_secs(5),
    };

    let error = runtime
        .block_on(restore_postgres_database(&executor, &container, &options))
        .expect_err("administrator-owned restore");

    assert_eq!(
        error.to_string(),
        "PostgreSQL restore request does not match its owned migration target"
    );
    assert!(executor.request.lock().expect("restore request").is_none());

    std::fs::remove_dir_all(&root).expect("remove restore owner fixture");
}

#[test]
fn postgres_target_verification_accepts_owned_valid_catalog() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("PostgreSQL verification runtime");
    let checkpoint = target_checkpoint(MigrationPhase::DataRestored);
    let credential = project_credential();
    let container = owned_container();
    let executor = RecordingExecutor::new(
        b"stackctl_bill_database_restore\tstackctl_bill_database_role\t0\t0\n".to_vec(),
        0,
    );
    let options = PostgresVerifyTargetOptions {
        checkpoint: &checkpoint,
        credential: &credential,
        installation_id: "install-1",
        target_database_name: "stackctl_bill_database_restore",
        target_role_name: "stackctl_bill_database_role",
        timeout: Duration::from_secs(5),
    };

    runtime
        .block_on(verify_postgres_target(&executor, &container, &options))
        .expect("verified PostgreSQL target");
}

#[test]
fn postgres_target_verification_rejects_wrong_owner_or_invalid_catalog() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("PostgreSQL verification runtime");
    let checkpoint = target_checkpoint(MigrationPhase::DataRestored);
    let credential = project_credential();
    let container = owned_container();
    let executor = RecordingExecutor::new(
        b"stackctl_bill_database_restore\tstackctl_admin\t1\t0\n".to_vec(),
        0,
    );
    let options = PostgresVerifyTargetOptions {
        checkpoint: &checkpoint,
        credential: &credential,
        installation_id: "install-1",
        target_database_name: "stackctl_bill_database_restore",
        target_role_name: "stackctl_bill_database_role",
        timeout: Duration::from_secs(5),
    };

    let error = runtime
        .block_on(verify_postgres_target(&executor, &container, &options))
        .expect_err("invalid PostgreSQL target");

    assert_eq!(
        error.to_string(),
        "PostgreSQL target catalog verification returned unexpected evidence"
    );
}

#[test]
fn postgres_target_provisioning_returns_the_deterministic_database_identity() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("PostgreSQL target runtime");
    let checkpoint = backup_checkpoint();
    let target = logical_resource();
    let target_credential = project_credential();
    let plan = PostgresLogicalResourcePlan::new(
        "bill",
        "database",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("target logical plan");
    assert!(plan.matches_credential(&target_credential));
    let mismatched_credential = CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/postgresql".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "stackctl_bill_database_role".to_owned(),
        secret: "different-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    });
    assert!(!plan.matches_credential(&mismatched_credential));
    let administrator = credential();
    let container = owned_container();
    let executor = RecordingExecutor::new(Vec::new(), 0);
    let options = PostgresProvisionTargetOptions {
        checkpoint: &checkpoint,
        target_logical_resource: &target,
        plan: &plan,
        administrator: &administrator,
        installation_id: "install-1",
    };

    let target_id = runtime
        .block_on(provision_postgres_target(&executor, &container, &options))
        .expect("provision PostgreSQL target");

    assert_eq!(target_id, "stackctl_bill_database");
    assert!(
        String::from_utf8(executor.input.lock().expect("provisioning SQL").clone())
            .expect("provisioning SQL UTF-8")
            .contains("CREATE DATABASE stackctl_bill_database")
    );
}

#[test]
fn postgres_migration_adapter_returns_owned_deterministic_target_state() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("PostgreSQL migration runtime");
    let checkpoint = backup_checkpoint();
    let source = migration_source_logical_resource();
    let target = logical_resource();
    let target_credential = project_credential();
    let plan = PostgresLogicalResourcePlan::new(
        "bill",
        "database",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("target logical plan");
    let administrator = credential();
    let source_container = owned_container_with_id("postgres-source");
    let target_container = owned_container_with_id("postgres-target");
    let executor = RecordingExecutor::new(Vec::new(), 0);
    let mut retirement = NoopPostgresSourceRetirement;
    let backup_root = backup_root("migration-adapter");
    let cutover =
        MigrationCutoverPlan::new(cutover_project(), cutover_environment()).expect("cutover plan");
    let rollback = MigrationRollbackPlan::new(
        rollback_project(),
        rollback_environment(),
        vec![retained_logical_resource()],
    )
    .expect("rollback plan");
    let options = PostgresMigrationOperationsOptions {
        source_container: &source_container,
        target_container: &target_container,
        source_logical_resource: &source,
        target_logical_resource: &target,
        source_credential: &target_credential,
        target_credential: &target_credential,
        target_plan: &plan,
        administrator: &administrator,
        installation_id: "install-1",
        source_database_name: "stackctl_bill_database",
        backup_root: &backup_root,
        operation_unix_seconds: 50_001,
        timeout: Duration::from_secs(30),
        cutover,
        rollback,
    };
    let mut operations = PostgresMigrationOperations::new(&executor, &mut retirement, options)
        .expect("PostgreSQL migration operations");

    let provisioned = runtime
        .block_on(operations.provision_target(&checkpoint))
        .expect("provision migration target");

    assert_eq!(provisioned.target_resource_id(), "stackctl_bill_database");
    assert_eq!(provisioned.logical_resource(), &target);
    assert_eq!(provisioned.credential(), &target_credential);
}

#[cfg(unix)]
#[test]
fn postgres_migration_adapter_runs_reversibly_before_explicit_retirement() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("PostgreSQL migration runtime");
    let root = backup_root("migration-execution");
    std::fs::create_dir_all(&root).expect("create PostgreSQL migration root");
    let database_path = root.join("state.sqlite3");
    let source = migration_source_logical_resource();
    let target = logical_resource();
    let target_credential = project_credential();
    let plan = PostgresLogicalResourcePlan::new(
        "bill",
        "database",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("target logical plan");
    let administrator = credential();
    let source_container = owned_container_with_id("postgres-source");
    let target_container = owned_container_with_id("postgres-target");
    let executor = RoutingPostgresExecutor;
    let mut retirement = RecordingPostgresSourceRetirement::default();
    let retirement_called = Arc::clone(&retirement.called);
    let cutover =
        MigrationCutoverPlan::new(cutover_project(), cutover_environment()).expect("cutover plan");
    let rollback = MigrationRollbackPlan::new(
        rollback_project(),
        rollback_environment(),
        vec![retained_logical_resource()],
    )
    .expect("rollback plan");
    let options = PostgresMigrationOperationsOptions {
        source_container: &source_container,
        target_container: &target_container,
        source_logical_resource: &source,
        target_logical_resource: &target,
        source_credential: &target_credential,
        target_credential: &target_credential,
        target_plan: &plan,
        administrator: &administrator,
        installation_id: "install-1",
        source_database_name: "stackctl_bill_database",
        backup_root: &root,
        operation_unix_seconds: 50_001,
        timeout: Duration::from_secs(30),
        cutover,
        rollback,
    };
    let inventory = adapter_inventory();
    let mut store = SqliteStateStore::open(&database_path).expect("open migration state");
    store
        .replace_project(&rollback_project())
        .expect("persist project");
    store
        .replace_managed_environment(&rollback_environment())
        .expect("persist source environment");
    store
        .upsert_logical_resources(std::slice::from_ref(&source))
        .expect("persist source logical resource");
    let mut operations = PostgresMigrationOperations::new(&executor, &mut retirement, options)
        .expect("PostgreSQL migration operations");

    let result = runtime
        .block_on(execute_migration(
            &mut store,
            &inventory,
            &mut operations,
            50_001,
        ))
        .expect("execute PostgreSQL migration");

    assert_eq!(result, MigrationExecutionResult::AwaitingConfirmation);
    assert_eq!(
        store.migrations().expect("cutover checkpoint")[0].phase(),
        MigrationPhase::Cutover
    );
    assert_eq!(
        store.managed_environments().expect("cutover environment"),
        vec![cutover_environment()]
    );
    assert!(!retirement_called.load(Ordering::Acquire));

    let result = runtime
        .block_on(confirm_migration(
            &mut store,
            &inventory,
            &mut operations,
            50_002,
        ))
        .expect("confirm PostgreSQL migration");

    assert_eq!(result, MigrationExecutionResult::Confirmed);
    assert!(retirement_called.load(Ordering::Acquire));

    drop(operations);
    drop(store);
    std::fs::remove_dir_all(root).expect("remove PostgreSQL migration root");
}

#[test]
fn postgres_source_retirement_retains_the_confirmed_logical_source() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("PostgreSQL retirement runtime");
    let inventory = adapter_inventory();
    let checkpoint = adapter_cutover_checkpoint();
    let source = retained_migration_source(&logical_resource(), "migration-bill-database");
    let source_container = owned_container_with_id("postgres-source");
    let administrator = credential();
    let source_credential = project_credential();
    let source_environment = rollback_environment();
    let executor = RecordingExecutor::new(Vec::new(), 0);
    let options = PostgresSourceRetirementOptions {
        source_container: &source_container,
        administrator: &administrator,
        source_credential: &source_credential,
        source_environment: &source_environment,
        installation_id: "install-1",
        timeout: Duration::from_secs(30),
    };
    let mut retirement =
        EnginePostgresSourceRetirement::new(&executor, options).expect("source retirement");

    runtime
        .block_on(retirement.retire_source(&inventory, &checkpoint, &source))
        .expect("retire PostgreSQL source");

    let sql = String::from_utf8(executor.input.lock().expect("retirement SQL").clone())
        .expect("retirement SQL UTF-8");
    assert!(sql.contains("ALTER ROLE stackctl_bill_database_role NOLOGIN"));
    assert!(!sql.contains("DROP DATABASE"));
    assert!(!sql.contains("DROP ROLE"));
}

#[test]
fn postgres_source_retirement_rejects_an_unconfirmed_checkpoint() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("PostgreSQL retirement runtime");
    let inventory = adapter_inventory();
    let checkpoint = backup_checkpoint();
    let source = retained_migration_source(&logical_resource(), "migration-bill-database");
    let source_container = owned_container_with_id("postgres-source");
    let administrator = credential();
    let source_credential = project_credential();
    let source_environment = rollback_environment();
    let executor = RecordingExecutor::new(Vec::new(), 0);
    let options = PostgresSourceRetirementOptions {
        source_container: &source_container,
        administrator: &administrator,
        source_credential: &source_credential,
        source_environment: &source_environment,
        installation_id: "install-1",
        timeout: Duration::from_secs(30),
    };
    let mut retirement =
        EnginePostgresSourceRetirement::new(&executor, options).expect("source retirement");

    let error = runtime
        .block_on(retirement.retire_source(&inventory, &checkpoint, &source))
        .expect_err("unconfirmed retirement must fail");

    assert!(error.to_string().contains("confirmed cutover source"));
    assert_eq!(*executor.request.lock().expect("recorded request"), None);
    assert!(executor.input.lock().expect("retirement SQL").is_empty());
}

struct NoopPostgresSourceRetirement;

impl PostgresSourceRetirement for NoopPostgresSourceRetirement {
    fn retire_source<'operation>(
        &'operation mut self,
        _inventory: &'operation MigrationRecord,
        _checkpoint: &'operation MigrationRecord,
        _source: &'operation LogicalResourceRecord,
    ) -> MigrationFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Default)]
struct RecordingPostgresSourceRetirement {
    called: Arc<AtomicBool>,
}

impl PostgresSourceRetirement for RecordingPostgresSourceRetirement {
    fn retire_source<'operation>(
        &'operation mut self,
        _inventory: &'operation MigrationRecord,
        checkpoint: &'operation MigrationRecord,
        source: &'operation LogicalResourceRecord,
    ) -> MigrationFuture<'operation, ()> {
        assert_eq!(checkpoint.phase(), MigrationPhase::Cutover);
        assert_eq!(source.logical_resource_id(), "bill/database");
        self.called.store(true, Ordering::Release);
        Box::pin(async { Ok(()) })
    }
}

struct RoutingPostgresExecutor;

impl CommandExecutor for RoutingPostgresExecutor {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        let output = match request.arguments().first().map(String::as_str) {
            Some("pg_dump") => b"portable-custom-format-dump".to_vec(),
            Some("psql")
                if request.arguments().iter().any(|argument| {
                    argument.starts_with("--command=SELECT current_database()")
                }) =>
            {
                b"stackctl_bill_database\tstackctl_bill_database_role\t0\t0\n".to_vec()
            }
            _ => Vec::new(),
        };
        let container_id = container.id().clone();

        Box::pin(async move {
            let (writer, mut reader) = duplex(64 * 1024);
            tokio::spawn(async move {
                tokio::io::copy(&mut reader, &mut tokio::io::sink())
                    .await
                    .expect("drain migration command input");
            });
            let output: ContainerLogStream<'static> =
                Box::pin(stream::iter(vec![Ok(LogChunk::stdout(output))]));

            Ok(CommandSession::new(
                CommandExecutionId::new("postgres-migration"),
                container_id,
                Box::pin(writer),
                output,
            ))
        })
    }

    fn command_status<'operation>(
        &'operation self,
        _execution_id: &'operation CommandExecutionId,
        _container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus> {
        Box::pin(async { Ok(CommandStatus::Exited(0)) })
    }
}

fn cutover_project() -> ProjectRecord {
    ProjectRecord::new(
        PathBuf::from("/work/bill"),
        "bill".to_owned(),
        vec!["bill-app.stackctl.localhost".to_owned()],
    )
}

fn rollback_project() -> ProjectRecord {
    cutover_project()
}

fn cutover_environment() -> ManagedEnvironmentRecord {
    migration_environment("sha256:environment-target", "stackctl_bill_database")
}

fn rollback_environment() -> ManagedEnvironmentRecord {
    migration_environment("sha256:environment-source", "source_bill")
}

fn migration_environment(revision: &str, database: &str) -> ManagedEnvironmentRecord {
    ManagedEnvironmentRecord::new(ManagedEnvironmentRecordOptions {
        project_id: "bill".to_owned(),
        revision: revision.to_owned(),
        values: BTreeMap::from([("DB_DATABASE".to_owned(), database.to_owned())]),
        lifecycle: EnvironmentLifecycle::Active,
    })
}

fn retained_logical_resource() -> LogicalResourceRecord {
    LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/database".to_owned(),
        shared_resource_id: "postgres-shared-17".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Retained,
        orphaned_at_unix_seconds: None,
    })
}

struct RecordingExecutor {
    request: Mutex<Option<CommandRequest>>,
    input: Arc<Mutex<Vec<u8>>>,
    input_complete: Arc<AtomicBool>,
    dump: Vec<u8>,
    exit_status: i64,
}

impl RecordingExecutor {
    fn new(dump: Vec<u8>, exit_status: i64) -> Self {
        Self {
            request: Mutex::new(None),
            input: Arc::new(Mutex::new(Vec::new())),
            input_complete: Arc::new(AtomicBool::new(false)),
            dump,
            exit_status,
        }
    }
}

impl CommandExecutor for RecordingExecutor {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        *self.request.lock().expect("request lock") = Some(request.clone());
        let dump = self.dump.clone();
        let captured_input = Arc::clone(&self.input);
        let input_complete = Arc::clone(&self.input_complete);
        let container_id = container.id().clone();

        Box::pin(async move {
            let (writer, mut reader) = duplex(1_024);
            tokio::spawn(async move {
                let mut input = Vec::new();
                reader
                    .read_to_end(&mut input)
                    .await
                    .expect("drain command stdin");
                *captured_input.lock().expect("captured command input") = input;
                input_complete.store(true, Ordering::Release);
            });
            let output: ContainerLogStream<'static> =
                Box::pin(stream::iter(vec![Ok(LogChunk::stdout(dump))]));

            Ok(CommandSession::new(
                CommandExecutionId::new("postgres-backup"),
                container_id,
                Box::pin(writer),
                output,
            ))
        })
    }

    fn command_status<'operation>(
        &'operation self,
        _execution_id: &'operation CommandExecutionId,
        _container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus> {
        Box::pin(async move {
            while !self.input_complete.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }

            Ok(CommandStatus::Exited(self.exit_status))
        })
    }
}

fn logical_resource() -> LogicalResourceRecord {
    LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/database".to_owned(),
        shared_resource_id: "postgres-shared-17".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
}

fn migration_source_logical_resource() -> LogicalResourceRecord {
    LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/database".to_owned(),
        shared_resource_id: "source:bill/database".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "database".to_owned(),
        kind: "postgres_database_and_role".to_owned(),
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        desired_revision: "sha256:source".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
}

fn credential() -> CredentialRecord {
    CredentialRecord::new(CredentialRecordOptions {
        credential_id: "shared/postgres/bootstrap".to_owned(),
        project_id: None,
        service_id: "postgresql".to_owned(),
        username: "stackctl_admin".to_owned(),
        secret: "do-not-log".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    })
}

fn project_credential() -> CredentialRecord {
    CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/database/postgresql".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "database".to_owned(),
        username: "stackctl_bill_database_role".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    })
}

fn restore_checkpoint(reference: &str, checksum: &str, size: u64) -> MigrationRecord {
    MigrationRecord::new(MigrationRecordOptions {
        migration_id: "migration-bill-database".to_owned(),
        project_id: "bill".to_owned(),
        source_revision: "sha256:source".to_owned(),
        target_revision: "sha256:target".to_owned(),
        source_compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        target_compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        phase: MigrationPhase::TargetProvisioned,
        backup_reference: Some(reference.to_owned()),
        backup_artifact_sha256: Some(checksum.to_owned()),
        backup_artifact_size_bytes: Some(size),
        target_resource_id: Some("stackctl_bill_database_restore".to_owned()),
        rollback_reference: Some("source:bill/database".to_owned()),
        updated_at_unix_seconds: 47_000,
    })
    .expect("restore checkpoint")
}

fn target_checkpoint(phase: MigrationPhase) -> MigrationRecord {
    MigrationRecord::new(MigrationRecordOptions {
        migration_id: "migration-bill-database".to_owned(),
        project_id: "bill".to_owned(),
        source_revision: "sha256:source".to_owned(),
        target_revision: "sha256:target".to_owned(),
        source_compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        target_compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        phase,
        backup_reference: Some("/private/backups/bill".to_owned()),
        backup_artifact_sha256: Some(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        ),
        backup_artifact_size_bytes: Some(13),
        target_resource_id: Some("stackctl_bill_database_restore".to_owned()),
        rollback_reference: Some("source:bill/database".to_owned()),
        updated_at_unix_seconds: 50_000,
    })
    .expect("target checkpoint")
}

fn backup_checkpoint() -> MigrationRecord {
    MigrationRecord::new(MigrationRecordOptions {
        migration_id: "migration-bill-database".to_owned(),
        project_id: "bill".to_owned(),
        source_revision: "sha256:source".to_owned(),
        target_revision: "sha256:target".to_owned(),
        source_compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        target_compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        phase: MigrationPhase::BackupVerified,
        backup_reference: Some("/private/backups/bill".to_owned()),
        backup_artifact_sha256: Some(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        ),
        backup_artifact_size_bytes: Some(13),
        target_resource_id: None,
        rollback_reference: Some("source:bill/database".to_owned()),
        updated_at_unix_seconds: 50_000,
    })
    .expect("backup checkpoint")
}

fn adapter_inventory() -> MigrationRecord {
    MigrationRecord::new(MigrationRecordOptions {
        migration_id: "migration-bill-database".to_owned(),
        project_id: "bill".to_owned(),
        source_revision: "sha256:source".to_owned(),
        target_revision: "sha256:target".to_owned(),
        source_compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        target_compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        phase: MigrationPhase::Inventoried,
        backup_reference: None,
        backup_artifact_sha256: None,
        backup_artifact_size_bytes: None,
        target_resource_id: None,
        rollback_reference: Some("source:bill/database".to_owned()),
        updated_at_unix_seconds: 50_000,
    })
    .expect("PostgreSQL migration inventory")
}

fn adapter_cutover_checkpoint() -> MigrationRecord {
    MigrationRecord::new(MigrationRecordOptions {
        migration_id: "migration-bill-database".to_owned(),
        project_id: "bill".to_owned(),
        source_revision: "sha256:source".to_owned(),
        target_revision: "sha256:target".to_owned(),
        source_compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        target_compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        phase: MigrationPhase::Cutover,
        backup_reference: Some("/private/backups/bill".to_owned()),
        backup_artifact_sha256: Some(
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        ),
        backup_artifact_size_bytes: Some(13),
        target_resource_id: Some("stackctl_bill_database".to_owned()),
        rollback_reference: Some("source:bill/database".to_owned()),
        updated_at_unix_seconds: 50_001,
    })
    .expect("PostgreSQL cutover checkpoint")
}

fn owned_container() -> OwnedContainer {
    owned_container_with_id("postgres-source")
}

fn owned_container_with_id(container_id: &str) -> OwnedContainer {
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::SharedService,
        project_id: None,
        compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("PostgreSQL metadata");

    let observed = ObservedContainer::new(ContainerId::new(container_id), metadata.labels());

    reconstruct_owned_container(&observed, "install-1", 8).expect("owned PostgreSQL container")
}

#[cfg(unix)]
fn contains_manifest(root: &Path) -> bool {
    let Ok(resource_directories) = std::fs::read_dir(root) else {
        return false;
    };

    resource_directories
        .filter_map(Result::ok)
        .flat_map(|entry| std::fs::read_dir(entry.path()).into_iter().flatten())
        .filter_map(Result::ok)
        .any(|entry| entry.path().join("manifest.json").is_file())
}

#[cfg(unix)]
fn backup_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "stackctl-postgres-backup-{label}-{}",
        std::process::id()
    ))
}
