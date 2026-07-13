use super::{PostgresBackupOptions, backup_postgres_database};
use crate::control_plane::engine::{
    CommandExecutionId, CommandExecutor, CommandRequest, CommandSession, CommandStatus,
    ContainerId, ContainerLogStream, EngineFuture, LogChunk, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ObservedContainer, OwnedContainer, ResourceKind,
    RetentionClass, reconstruct_owned_container,
};
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
    LogicalResourceRecordOptions, ResourceLifecycle,
};
use futures_util::stream;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;
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

struct RecordingExecutor {
    request: Mutex<Option<CommandRequest>>,
    dump: Vec<u8>,
    exit_status: i64,
}

impl RecordingExecutor {
    fn new(dump: Vec<u8>, exit_status: i64) -> Self {
        Self {
            request: Mutex::new(None),
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
        let container_id = container.id().clone();

        Box::pin(async move {
            let (writer, mut reader) = duplex(1_024);
            tokio::spawn(async move {
                let mut input = Vec::new();
                reader
                    .read_to_end(&mut input)
                    .await
                    .expect("drain command stdin");
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
        Box::pin(async move { Ok(CommandStatus::Exited(self.exit_status)) })
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
