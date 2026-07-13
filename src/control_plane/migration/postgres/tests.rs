use super::{
    PostgresBackupOptions, PostgresRestoreOptions, backup_postgres_database,
    restore_postgres_database,
};
use crate::control_plane::engine::{
    CommandExecutionId, CommandExecutor, CommandRequest, CommandSession, CommandStatus,
    ContainerId, ContainerLogStream, EngineFuture, LogChunk, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ObservedContainer, OwnedContainer, ResourceKind,
    RetentionClass, reconstruct_owned_container,
};
use crate::control_plane::retention::{
    BackupResourceIdentity, store_backup_artifact_for_identity, verify_stored_backup_artifact,
};
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
    LogicalResourceRecordOptions, MigrationPhase, MigrationRecord, MigrationRecordOptions,
    ResourceLifecycle,
};
use futures_util::stream;
use std::collections::BTreeMap;
use std::path::Path;
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
        "PostgreSQL backup failed: dump PostgreSQL database exited with status 7"
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
        source_revision: "sha256:v7".to_owned(),
        target_revision: "sha256:v8".to_owned(),
        source_compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        target_compatibility_fingerprint: "sha256:postgres-17".to_owned(),
        phase: MigrationPhase::TargetProvisioned,
        backup_reference: Some(reference.to_owned()),
        backup_artifact_sha256: Some(checksum.to_owned()),
        backup_artifact_size_bytes: Some(size),
        target_resource_id: Some("stackctl_bill_database_restore".to_owned()),
        rollback_reference: Some("v7:bill/database".to_owned()),
        updated_at_unix_seconds: 47_000,
    })
    .expect("restore checkpoint")
}

fn owned_container() -> OwnedContainer {
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

    let observed = ObservedContainer::new(ContainerId::new("postgres-source"), metadata.labels());

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
fn backup_root(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "stackctl-postgres-backup-{label}-{}",
        std::process::id()
    ))
}
