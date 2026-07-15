use super::{
    MinioRestoreOptions, backup_minio_bucket::validate_version_inventory, restore_minio_bucket,
};
use crate::control_plane::engine::{
    CommandExecutionId, CommandExecutor, CommandRequest, CommandSession, CommandStatus,
    ContainerId, ContainerLogStream, ContainerVolumeArchive, EngineFuture, LogChunk,
    ManagedResourceMetadata, ManagedResourceMetadataOptions, ObservedContainer, ObservedVolume,
    OwnedContainer, OwnedVolume, ResourceKind, RetentionClass, reconstruct_owned_container,
    reconstruct_owned_volume,
};
use crate::control_plane::retention::{
    BackupResourceIdentity, store_backup_artifact_for_identity, verify_stored_backup_artifact,
};
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
    LogicalResourceRecordOptions, RecoveryPointRecord, RecoveryPointRecordOptions,
    ResourceLifecycle,
};
use futures_util::stream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, duplex};

#[test]
fn minio_backup_rejects_version_history_it_cannot_preserve() {
    let error = validate_version_inventory(
        "stackctl-bill-files",
        br#"{"status":"success","versioning":{"status":"Enabled"}}"#,
    )
    .expect_err("versioned bucket must fail closed");

    assert!(error.to_string().contains("uses versioning"));
}

#[test]
fn minio_backup_accepts_an_unversioned_inventory() {
    validate_version_inventory(
        "stackctl-bill-files",
        br#"{"status":"success","versioning":{}}"#,
    )
    .expect("unversioned bucket");
}

#[test]
fn minio_restore_verifies_evidence_and_replaces_only_the_exact_bucket() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("MinIO restore runtime");
    let root = std::env::temp_dir().join(format!("stackctl-minio-restore-{}", std::process::id()));
    let logical = logical_resource();
    let identity = BackupResourceIdentity::from_logical(&logical, "install-1");
    let archive = b"verified MinIO tar archive";
    let stored = store_backup_artifact_for_identity(&identity, archive, 47_000, &root)
        .expect("stored MinIO restore fixture");
    let evidence = verify_stored_backup_artifact(&stored, 47_001).expect("MinIO evidence");
    let recovery = recovery_point(
        stored.recovery_point().to_str().expect("backup reference"),
        evidence.artifact_sha256(),
        evidence.artifact_size_bytes(),
        47_000,
    );
    let credential = credential();
    let container = owned_container();
    let volume = owned_volume();
    let executor = RecordingExecutor::new();

    runtime
        .block_on(restore_minio_bucket(
            &executor,
            &container,
            &volume,
            &MinioRestoreOptions {
                recovery_point: &recovery,
                logical_resource: &logical,
                credential: &credential,
                installation_id: "install-1",
                target_bucket_name: "stackctl-bill-files",
                verified_at_unix_seconds: 47_001,
                timeout: Duration::from_secs(30),
            },
        ))
        .expect("restore MinIO bucket");

    assert_eq!(*executor.archive.lock().expect("restore archive"), archive);
    let request = executor
        .request
        .lock()
        .expect("restore request")
        .clone()
        .expect("executed restore request");
    assert_eq!(request.arguments()[..2], ["sh", "-c"]);
    assert!(request.arguments()[2].contains("mc --config-dir"));
    assert!(request.arguments()[2].contains("mirror --overwrite --remove"));
    assert!(!request.arguments()[2].contains("tar"));
    assert_eq!(
        request.environment()["STACKCTL_BUCKET"],
        "stackctl-bill-files"
    );
    assert_eq!(
        request.environment()["STACKCTL_ACCESS_KEY"],
        "st_bill_files"
    );
    assert!(!format!("{:?}", request.arguments()).contains("project-secret"));
    assert_eq!(
        executor
            .archive_path
            .lock()
            .expect("restore archive path")
            .as_deref(),
        Some(std::path::Path::new(
            ".stackctl-minio-export/stackctl-bill-files-47000"
        ))
    );

    std::fs::remove_dir_all(root).expect("remove MinIO restore fixture");
}

#[test]
fn minio_restore_rejects_checkpoint_mismatch_before_target_command() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("MinIO restore runtime");
    let root = std::env::temp_dir().join(format!(
        "stackctl-minio-restore-mismatch-{}",
        std::process::id()
    ));
    let logical = logical_resource();
    let identity = BackupResourceIdentity::from_logical(&logical, "install-1");
    let stored = store_backup_artifact_for_identity(&identity, b"verified archive", 48_000, &root)
        .expect("stored MinIO mismatch fixture");
    let recovery = recovery_point(
        stored.recovery_point().to_str().expect("backup reference"),
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        16,
        48_000,
    );
    let credential = credential();
    let container = owned_container();
    let volume = owned_volume();
    let executor = RecordingExecutor::new();

    let error = runtime
        .block_on(restore_minio_bucket(
            &executor,
            &container,
            &volume,
            &MinioRestoreOptions {
                recovery_point: &recovery,
                logical_resource: &logical,
                credential: &credential,
                installation_id: "install-1",
                target_bucket_name: "stackctl-bill-files",
                verified_at_unix_seconds: 48_001,
                timeout: Duration::from_secs(30),
            },
        ))
        .expect_err("mismatched MinIO checkpoint");

    assert_eq!(
        error.to_string(),
        "MinIO restore backup does not match its recovery point"
    );
    assert!(executor.request.lock().expect("restore request").is_none());

    std::fs::remove_dir_all(root).expect("remove MinIO mismatch fixture");
}

struct RecordingExecutor {
    request: Mutex<Option<CommandRequest>>,
    input: Arc<Mutex<Vec<u8>>>,
    input_complete: Arc<AtomicBool>,
    archive: Arc<Mutex<Vec<u8>>>,
    archive_path: Mutex<Option<std::path::PathBuf>>,
}

impl RecordingExecutor {
    fn new() -> Self {
        Self {
            request: Mutex::new(None),
            input: Arc::new(Mutex::new(Vec::new())),
            input_complete: Arc::new(AtomicBool::new(false)),
            archive: Arc::new(Mutex::new(Vec::new())),
            archive_path: Mutex::new(None),
        }
    }
}

impl CommandExecutor for RecordingExecutor {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        if request.environment().contains_key("STACKCTL_BUCKET") {
            *self.request.lock().expect("request lock") = Some(request.clone());
        }
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
                    .expect("drain MinIO restore input");
                *captured_input.lock().expect("captured input") = input;
                input_complete.store(true, Ordering::Release);
            });
            let output: ContainerLogStream<'static> =
                Box::pin(stream::iter(vec![Ok(LogChunk::stdout(Vec::new()))]));
            Ok(CommandSession::new(
                CommandExecutionId::new("minio-restore"),
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
            Ok(CommandStatus::Exited(0))
        })
    }
}

impl ContainerVolumeArchive for RecordingExecutor {
    fn download_volume_archive<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
        _volume: &'operation OwnedVolume,
        _output: &'operation mut (dyn tokio::io::AsyncWrite + Send + Unpin),
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async { unreachable!("whole-volume download is not used by MinIO restore") })
    }

    fn upload_volume_archive<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
        _volume: &'operation OwnedVolume,
        _archive: &'operation std::path::Path,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async { unreachable!("whole-volume upload is not used by MinIO restore") })
    }

    fn download_volume_subpath_archive<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
        _volume: &'operation OwnedVolume,
        _relative_path: &'operation std::path::Path,
        _output: &'operation mut (dyn tokio::io::AsyncWrite + Send + Unpin),
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async { unreachable!("subpath download is not used by MinIO restore") })
    }

    fn upload_volume_subpath_archive<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
        _volume: &'operation OwnedVolume,
        relative_path: &'operation std::path::Path,
        mut archive: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
    ) -> EngineFuture<'operation, ()> {
        *self.archive_path.lock().expect("restore archive path") =
            Some(relative_path.to_path_buf());
        let captured = Arc::clone(&self.archive);
        Box::pin(async move {
            let mut bytes = Vec::new();
            archive
                .read_to_end(&mut bytes)
                .await
                .expect("read MinIO restore archive");
            *captured.lock().expect("capture MinIO restore archive") = bytes;
            Ok(())
        })
    }
}

fn logical_resource() -> LogicalResourceRecord {
    LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: "bill/files/object-store".to_owned(),
        shared_resource_id: "minio-shared".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "files".to_owned(),
        kind: "minio_bucket_policy".to_owned(),
        compatibility_fingerprint: "sha256:minio-2025".to_owned(),
        desired_revision: "sha256:desired".to_owned(),
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
}

fn credential() -> CredentialRecord {
    CredentialRecord::new(CredentialRecordOptions {
        credential_id: "bill/files/object-store".to_owned(),
        project_id: Some("bill".to_owned()),
        service_id: "files".to_owned(),
        username: "st_bill_files".to_owned(),
        secret: "project-secret".to_owned(),
        lifecycle: CredentialLifecycle::Active,
    })
}

fn recovery_point(
    reference: &str,
    checksum: &str,
    size: u64,
    created_at_unix_seconds: i64,
) -> RecoveryPointRecord {
    RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "backup-bill-files".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "files".to_owned(),
        logical_resource_id: "bill/files/object-store".to_owned(),
        resource_kind: "minio_bucket_policy".to_owned(),
        compatibility_fingerprint: "sha256:minio-2025".to_owned(),
        reference: reference.to_owned(),
        artifact_sha256: checksum.to_owned(),
        artifact_size_bytes: size,
        created_at_unix_seconds,
        verified_at_unix_seconds: created_at_unix_seconds,
    })
    .expect("MinIO recovery point")
}

fn owned_container() -> OwnedContainer {
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::SharedService,
        project_id: None,
        compatibility_fingerprint: "sha256:minio-2025".to_owned(),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("MinIO metadata");
    let observed = ObservedContainer::new(ContainerId::new("minio-target"), metadata.labels());

    reconstruct_owned_container(&observed, "install-1", 8).expect("owned MinIO container")
}

fn owned_volume() -> OwnedVolume {
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::Volume,
        project_id: None,
        compatibility_fingerprint: "sha256:minio-2025".to_owned(),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("MinIO volume metadata");
    let observed = ObservedVolume::new("minio-shared", metadata.labels());

    reconstruct_owned_volume(&observed, "install-1", 8).expect("owned MinIO volume")
}
