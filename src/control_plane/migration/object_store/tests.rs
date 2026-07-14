use super::{
    MinioRestoreOptions, V7MinioCredential, V7MinioMigrationProvider,
    V7MinioMigrationProviderOptions, V7MinioSourceRetirement,
    backup_minio_bucket::validate_version_inventory, restore_minio_bucket,
};
use crate::control_plane::engine::{
    CommandExecutionId, CommandExecutor, CommandRequest, CommandSession, CommandStatus,
    ContainerId, ContainerLogStream, EngineFuture, LogChunk, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ObservedContainer, ResourceKind, RetentionClass,
    V7ContainerCommandExecutor, V7ContainerCommandTarget, reconstruct_owned_container,
};
use crate::control_plane::migration::{
    MigrationFuture, V7LogicalDataMigrationSource, V7LogicalDataMigrationSourceOptions,
    V7MigrationAdapterTarget, V7RecoverableMigrationProvider,
};
use crate::control_plane::retention::{
    BackupResourceIdentity, store_backup_artifact_for_identity, verify_stored_backup_artifact,
};
use crate::control_plane::shared_infrastructure::{CredentialSecret, ObjectStoreProjectDefinition};
use crate::control_plane::state::{
    AcceptedV7InventoryRecord, AcceptedV7InventoryRecordOptions, CredentialLifecycle,
    CredentialRecord, CredentialRecordOptions, LogicalResourceRecord, LogicalResourceRecordOptions,
    RecoveryPointRecord, RecoveryPointRecordOptions, ResourceLifecycle,
    V7MigrationAdapterCheckpoint,
};
use futures_util::stream;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, duplex};

#[cfg(unix)]
#[test]
fn v7_minio_provider_recovers_replays_and_retires_only_after_confirmation() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("v7 MinIO provider runtime");
    let root = backup_root("v7-minio-provider");
    let archive = b"accepted v7 MinIO tar archive";
    let executor = ProviderExecutor::new(archive.to_vec());
    let accepted = accepted_inventory();
    let source = v7_source();
    let source_credential =
        V7MinioCredential::new("legacy-access", "legacy-secret").expect("source credential");
    let logical = logical_resource();
    let credential = credential();
    let definition = ObjectStoreProjectDefinition::new(
        "bill",
        "files",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("target definition");
    let container = owned_container();
    let mut retirement = ProviderRetirement::default();
    let retired = Arc::clone(&retirement.called);
    let options = V7MinioMigrationProviderOptions {
        accepted: &accepted,
        source: &source,
        source_credential: &source_credential,
        target_container: &container,
        target_logical_resource: &logical,
        target_credential: &credential,
        target_definition: &definition,
        installation_id: "install-1",
        backup_root: &root,
        created_at_unix_seconds: 66_000,
        verified_at_unix_seconds: 66_001,
        timeout: Duration::from_secs(30),
    };
    let debug = format!("{options:?}");
    assert!(!debug.contains("legacy-access"));
    assert!(!debug.contains("legacy-secret"));
    assert!(!debug.contains("project-secret"));
    let mut provider = V7MinioMigrationProvider::new(&executor, &mut retirement, options)
        .expect("v7 MinIO provider");

    let backup = runtime
        .block_on(provider.backup_source(&source))
        .expect("backup source bucket");
    assert_eq!(
        std::fs::read(Path::new(backup.reference()).join("artifact.bin")).expect("backup artifact"),
        archive
    );
    {
        let requests = executor.v7_requests.lock().expect("v7 requests");
        assert_eq!(requests.len(), 2);
        assert!(requests[0].arguments()[2].contains("version info"));
        assert!(requests[1].arguments()[2].contains("mc --config-dir"));
        assert_eq!(requests[1].environment()["STACKCTL_BUCKET"], "media");
        assert_eq!(
            requests[1].environment()["STACKCTL_SECRET_KEY"],
            "legacy-secret"
        );
        assert!(!format!("{:?}", requests[1].arguments()).contains("legacy-secret"));
    }
    let checkpoint =
        V7MigrationAdapterCheckpoint::pending("service/files", "minio-bucket", true, 65_999)
            .expect("pending checkpoint")
            .with_recovery_verified(
                backup.reference(),
                backup.artifact_sha256(),
                backup.artifact_size_bytes(),
                66_001,
            )
            .expect("verified checkpoint");
    let artifact = Path::new(backup.reference()).join("artifact.bin");
    std::fs::write(&artifact, b"tampered").expect("tamper backup");
    let error = runtime
        .block_on(provider.restore_and_verify_target(&source, &checkpoint))
        .expect_err("tampered recovery must block target mutation");
    assert!(error.to_string().contains("checksum does not match"));
    assert_eq!(executor.v8_starts.load(Ordering::Acquire), 0);
    std::fs::write(&artifact, archive).expect("restore fixture");

    let expected =
        V7MigrationAdapterTarget::resource("minio:bill/files/object-store:stackctl-bill-files")
            .expect("target reference");
    assert_eq!(
        runtime
            .block_on(provider.restore_and_verify_target(&source, &checkpoint))
            .expect("restore target"),
        expected
    );
    assert_eq!(
        runtime
            .block_on(provider.restore_and_verify_target(&source, &checkpoint))
            .expect("replay restore"),
        expected
    );
    assert_eq!(executor.v8_starts.load(Ordering::Acquire), 4);
    let restored = executor.restored_inputs.lock().expect("restored inputs");
    assert_eq!(
        restored.as_slice(),
        [archive.as_slice(), archive.as_slice()]
    );
    drop(restored);
    runtime
        .block_on(
            provider.verify_target(&source, "minio:bill/files/object-store:stackctl-bill-files"),
        )
        .expect("verify target");
    runtime
        .block_on(provider.verify_source(&source))
        .expect("verify source");
    assert!(!retired.load(Ordering::Acquire));
    runtime
        .block_on(provider.retire_source(&source))
        .expect("retire source");
    assert!(retired.load(Ordering::Acquire));

    drop(provider);
    std::fs::remove_dir_all(root).expect("remove fixture");
}

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
fn minio_definition_binds_the_exact_target_credential_secret() {
    let definition = ObjectStoreProjectDefinition::new(
        "bill",
        "files",
        CredentialSecret::new("project-secret".to_owned()),
    )
    .expect("target definition");
    let matching = credential();
    let mismatched = CredentialRecord::new(CredentialRecordOptions {
        credential_id: matching.credential_id().to_owned(),
        project_id: matching.project_id().map(str::to_owned),
        service_id: matching.service_id().to_owned(),
        username: matching.username().to_owned(),
        secret: "other-secret".to_owned(),
        lifecycle: matching.lifecycle(),
    });

    assert!(definition.matches_credential(&matching));
    assert!(!definition.matches_credential(&mismatched));
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
    let executor = RecordingExecutor::new();

    runtime
        .block_on(restore_minio_bucket(
            &executor,
            &container,
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

    assert_eq!(*executor.input.lock().expect("restore input"), archive);
    let request = executor
        .request
        .lock()
        .expect("restore request")
        .clone()
        .expect("executed restore request");
    assert_eq!(request.arguments()[..2], ["sh", "-c"]);
    assert!(request.arguments()[2].contains("mc --config-dir"));
    assert!(request.arguments()[2].contains("mirror --overwrite --remove"));
    assert_eq!(
        request.environment()["STACKCTL_BUCKET"],
        "stackctl-bill-files"
    );
    assert_eq!(
        request.environment()["STACKCTL_ACCESS_KEY"],
        "st_bill_files"
    );
    assert!(!format!("{:?}", request.arguments()).contains("project-secret"));

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
    let executor = RecordingExecutor::new();

    let error = runtime
        .block_on(restore_minio_bucket(
            &executor,
            &container,
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

struct ProviderExecutor {
    archive: Vec<u8>,
    v7_requests: Mutex<Vec<CommandRequest>>,
    restored_inputs: Arc<Mutex<Vec<Vec<u8>>>>,
    input_complete: Arc<AtomicBool>,
    v8_starts: AtomicUsize,
}

impl ProviderExecutor {
    fn new(archive: Vec<u8>) -> Self {
        Self {
            archive,
            v7_requests: Mutex::new(Vec::new()),
            restored_inputs: Arc::new(Mutex::new(Vec::new())),
            input_complete: Arc::new(AtomicBool::new(true)),
            v8_starts: AtomicUsize::new(0),
        }
    }

    fn start_session(
        &self,
        container_id: ContainerId,
        request: &CommandRequest,
        v7: bool,
    ) -> EngineFuture<'_, CommandSession> {
        let script = request
            .arguments()
            .get(2)
            .map(String::as_str)
            .unwrap_or_default();
        let captures_restore = !v7 && script.contains("mirror --overwrite --remove");
        let output = if script.contains("version info") {
            br#"{"status":"success","versioning":{}}"#.to_vec()
        } else if v7 && script.contains("tar -C") {
            self.archive.clone()
        } else if script.contains("stat") {
            format!("{}\n", request.environment()["STACKCTL_BUCKET"]).into_bytes()
        } else {
            Vec::new()
        };
        let restored_inputs = Arc::clone(&self.restored_inputs);
        let input_complete = Arc::clone(&self.input_complete);
        input_complete.store(false, Ordering::Release);

        Box::pin(async move {
            let (writer, mut reader) = duplex(64 * 1024);
            tokio::spawn(async move {
                let mut input = Vec::new();
                reader
                    .read_to_end(&mut input)
                    .await
                    .expect("drain MinIO provider command input");
                if captures_restore {
                    restored_inputs
                        .lock()
                        .expect("restored MinIO inputs")
                        .push(input);
                }
                input_complete.store(true, Ordering::Release);
            });
            let output: ContainerLogStream<'static> =
                Box::pin(stream::iter(vec![Ok(LogChunk::stdout(output))]));
            Ok(CommandSession::new(
                CommandExecutionId::new("v7-minio-provider"),
                container_id,
                Box::pin(writer),
                output,
            ))
        })
    }

    fn status(&self) -> EngineFuture<'_, CommandStatus> {
        Box::pin(async move {
            while !self.input_complete.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
            Ok(CommandStatus::Exited(0))
        })
    }
}

impl CommandExecutor for ProviderExecutor {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation crate::control_plane::engine::OwnedContainer,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        self.v8_starts.fetch_add(1, Ordering::AcqRel);
        self.start_session(container.id().clone(), request, false)
    }

    fn command_status<'operation>(
        &'operation self,
        _execution_id: &'operation CommandExecutionId,
        _container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus> {
        self.status()
    }
}

impl V7ContainerCommandExecutor for ProviderExecutor {
    fn start_v7_command<'operation>(
        &'operation self,
        target: &'operation V7ContainerCommandTarget,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        self.v7_requests
            .lock()
            .expect("v7 MinIO requests")
            .push(request.clone());
        self.start_session(target.container_id().clone(), request, true)
    }

    fn v7_command_status<'operation>(
        &'operation self,
        _execution_id: &'operation CommandExecutionId,
        _container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus> {
        self.status()
    }
}

#[derive(Default)]
struct ProviderRetirement {
    called: Arc<AtomicBool>,
}

impl V7MinioSourceRetirement for ProviderRetirement {
    fn retire_source<'operation>(
        &'operation mut self,
        source: &'operation V7LogicalDataMigrationSource,
    ) -> MigrationFuture<'operation, ()> {
        assert_eq!(source, &v7_source());
        self.called.store(true, Ordering::Release);
        Box::pin(async { Ok(()) })
    }
}

fn backup_root(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("stackctl-{name}-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&root).expect("create backup root");
    root
}

fn v7_source() -> V7LogicalDataMigrationSource {
    V7LogicalDataMigrationSource::new(V7LogicalDataMigrationSourceOptions {
        project_id: "bill".to_owned(),
        service_id: "files".to_owned(),
        kind: "object_store".to_owned(),
        driver: "minio".to_owned(),
        container_name: "bill-files".to_owned(),
        container_id: "legacy-minio".to_owned(),
        named_volumes: vec!["bill-files-data".to_owned()],
        logical_data: BTreeMap::from([("bucket".to_owned(), "media".to_owned())]),
    })
    .expect("v7 MinIO source")
}

fn accepted_inventory() -> AcceptedV7InventoryRecord {
    let source_revision = format!("sha256:{}", "c".repeat(64));
    let inventory_json = serde_json::json!({
        "project_id": "bill",
        "canonical_project_path": "/work/bill",
        "source_revision": source_revision,
        "blockers": [],
        "services": [{
            "service_id": "files",
            "kind": "object_store",
            "driver": "minio",
            "configured_image": "minio/minio:latest",
            "container_name": "bill-files",
            "observed_container_id": "legacy-minio",
            "configured_mounts": [{
                "source_kind": "named_volume",
                "source": "bill-files-data",
                "target": "/data",
                "read_only": false
            }],
            "observed_mounts": [{
                "source_kind": "named_volume",
                "source": "bill-files-data",
                "target": "/data",
                "read_only": false
            }],
            "logical_data": {"bucket": "media"}
        }]
    })
    .to_string();
    AcceptedV7InventoryRecord::new(AcceptedV7InventoryRecordOptions {
        project_id: "bill".to_owned(),
        canonical_project_path: PathBuf::from("/work/bill"),
        source_revision,
        inventory_json,
        generated_environment_rollback: None,
        accepted_at_unix_seconds: 65_999,
    })
    .expect("accepted v7 MinIO inventory")
}

struct RecordingExecutor {
    request: Mutex<Option<CommandRequest>>,
    input: Arc<Mutex<Vec<u8>>>,
    input_complete: Arc<AtomicBool>,
}

impl RecordingExecutor {
    fn new() -> Self {
        Self {
            request: Mutex::new(None),
            input: Arc::new(Mutex::new(Vec::new())),
            input_complete: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl CommandExecutor for RecordingExecutor {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation crate::control_plane::engine::OwnedContainer,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        *self.request.lock().expect("request lock") = Some(request.clone());
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

fn owned_container() -> crate::control_plane::engine::OwnedContainer {
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
