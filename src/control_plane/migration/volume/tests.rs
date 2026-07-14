use super::{
    ProjectVolumeBackupOptions, ProjectVolumeRestoreOptions, backup_project_volume,
    restore_project_volume,
};
use crate::control_plane::engine::{
    ContainerCreateOptions, ContainerHealth, ContainerId, ContainerLifecycle, ContainerState,
    ContainerVolumeArchive, EngineError, EngineFuture, HealthObserver, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ObservedContainer, ObservedVolume, OwnedContainer, OwnedVolume,
    ResourceKind, RetentionClass, VolumeCreateOptions, VolumeManager, VolumeMount,
    reconstruct_owned_container, reconstruct_owned_volume,
};
use crate::control_plane::state::{
    RecoveryPointRecord, RecoveryPointRecordOptions, ResourceLifecycle, ResourceRecord,
    ResourceRecordOptions, ResourceRetention,
};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;

#[test]
fn owned_project_volume_backup_quiesces_streams_verifies_and_restarts() {
    let root = temporary_directory("success");
    let (container, volume, resource) = fixture();
    let mut engine = RecordingVolumeArchiveEngine::new(b"owned tar archive".to_vec());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");

    let backup = runtime
        .block_on(backup_project_volume(
            &mut engine,
            &container,
            &volume,
            &ProjectVolumeBackupOptions {
                resource: &resource,
                installation_id: "install-1",
                project_id: "bill",
                service_id: "search",
                created_at_unix_seconds: 50_000,
                backup_root: &root,
                timeout: Duration::from_secs(5),
            },
        ))
        .expect("verified volume backup");

    assert_eq!(backup.artifact_size_bytes(), 17);
    assert_eq!(
        backup.artifact_sha256(),
        hex::encode(Sha256::digest(b"owned tar archive"))
    );
    assert_eq!(engine.calls(), ["inspect", "stop", "archive", "start"]);
    std::fs::remove_dir_all(root).expect("remove volume backup fixture");
}

#[test]
fn owned_project_volume_backup_restarts_after_archive_failure() {
    let root = temporary_directory("failure");
    let (container, volume, resource) = fixture();
    let mut engine = RecordingVolumeArchiveEngine::new(Vec::new()).with_archive_failure();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(backup_project_volume(
            &mut engine,
            &container,
            &volume,
            &ProjectVolumeBackupOptions {
                resource: &resource,
                installation_id: "install-1",
                project_id: "bill",
                service_id: "search",
                created_at_unix_seconds: 50_001,
                backup_root: &root,
                timeout: Duration::from_secs(5),
            },
        ))
        .expect_err("archive failure must fail backup");

    assert!(error.to_string().contains("archive unavailable"));
    assert_eq!(engine.calls(), ["inspect", "stop", "archive", "start"]);
    if root.exists() {
        std::fs::remove_dir_all(root).expect("remove failed volume backup fixture");
    }
}

#[test]
fn owned_project_volume_restore_recreates_empty_target_before_upload() {
    let root = temporary_directory("restore");
    let (container, volume, resource) = fixture();
    let mut backup_engine = RecordingVolumeArchiveEngine::new(b"owned tar archive".to_vec());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");
    let backup = runtime
        .block_on(backup_project_volume(
            &mut backup_engine,
            &container,
            &volume,
            &ProjectVolumeBackupOptions {
                resource: &resource,
                installation_id: "install-1",
                project_id: "bill",
                service_id: "search",
                created_at_unix_seconds: 50_002,
                backup_root: &root,
                timeout: Duration::from_secs(5),
            },
        ))
        .expect("verified source backup");
    let recovery = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "restore-point".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "search".to_owned(),
        logical_resource_id: resource.resource_id().to_owned(),
        resource_kind: resource.kind().to_owned(),
        compatibility_fingerprint: resource.compatibility_fingerprint().to_owned(),
        reference: backup.reference().to_owned(),
        artifact_sha256: backup.artifact_sha256().to_owned(),
        artifact_size_bytes: backup.artifact_size_bytes(),
        created_at_unix_seconds: 50_002,
        verified_at_unix_seconds: 50_002,
    })
    .expect("cataloged volume recovery");
    let desired_volume =
        VolumeCreateOptions::new(volume.name(), volume.metadata().clone()).expect("desired volume");
    let desired_container = ContainerCreateOptions::new(
        "stackctl-bill-search",
        concat!(
            "search@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ),
        container.metadata().clone(),
    )
    .expect("desired container")
    .with_volume_mount(
        VolumeMount::read_write(volume.name(), "/var/lib/search").expect("volume mount"),
    );
    let mut restore_engine = RecordingVolumeRestoreEngine::default();

    runtime
        .block_on(restore_project_volume(
            &mut restore_engine,
            &container,
            &volume,
            &ProjectVolumeRestoreOptions {
                resource: &resource,
                recovery_point: &recovery,
                desired_container: &desired_container,
                desired_volume: &desired_volume,
                installation_id: "install-1",
                project_id: "bill",
                service_id: "search",
                verified_at_unix_seconds: 50_003,
                timeout: Duration::from_secs(5),
            },
        ))
        .expect("restored project volume");

    assert_eq!(
        restore_engine.calls(),
        [
            "inspect",
            "stop",
            "remove_container",
            "remove_volume",
            "create_volume",
            "create_container",
            "upload",
            "start",
            "inspect",
            "health",
        ]
    );
    assert_eq!(restore_engine.uploaded(), b"owned tar archive");
    std::fs::remove_dir_all(root).expect("remove restore fixture");
}

#[test]
fn owned_project_volume_restore_rejects_tampering_before_mutation() {
    let root = temporary_directory("restore-tampered");
    let (container, volume, resource) = fixture();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");
    let mut backup_engine = RecordingVolumeArchiveEngine::new(b"owned tar archive".to_vec());
    let backup = runtime
        .block_on(backup_project_volume(
            &mut backup_engine,
            &container,
            &volume,
            &ProjectVolumeBackupOptions {
                resource: &resource,
                installation_id: "install-1",
                project_id: "bill",
                service_id: "search",
                created_at_unix_seconds: 50_004,
                backup_root: &root,
                timeout: Duration::from_secs(5),
            },
        ))
        .expect("verified source backup");
    let recovery = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "tampered-point".to_owned(),
        project_id: "bill".to_owned(),
        service_id: "search".to_owned(),
        logical_resource_id: resource.resource_id().to_owned(),
        resource_kind: resource.kind().to_owned(),
        compatibility_fingerprint: resource.compatibility_fingerprint().to_owned(),
        reference: backup.reference().to_owned(),
        artifact_sha256: backup.artifact_sha256().to_owned(),
        artifact_size_bytes: backup.artifact_size_bytes(),
        created_at_unix_seconds: 50_004,
        verified_at_unix_seconds: 50_004,
    })
    .expect("cataloged volume recovery");
    std::fs::write(
        std::path::Path::new(backup.reference()).join("artifact.bin"),
        b"tampered archive",
    )
    .expect("tamper restore artifact");
    let desired_volume =
        VolumeCreateOptions::new(volume.name(), volume.metadata().clone()).expect("desired volume");
    let desired_container = ContainerCreateOptions::new(
        "stackctl-bill-search",
        concat!(
            "search@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ),
        container.metadata().clone(),
    )
    .expect("desired container")
    .with_volume_mount(
        VolumeMount::read_write(volume.name(), "/var/lib/search").expect("volume mount"),
    );
    let mut restore_engine = RecordingVolumeRestoreEngine::default();

    let error = runtime
        .block_on(restore_project_volume(
            &mut restore_engine,
            &container,
            &volume,
            &ProjectVolumeRestoreOptions {
                resource: &resource,
                recovery_point: &recovery,
                desired_container: &desired_container,
                desired_volume: &desired_volume,
                installation_id: "install-1",
                project_id: "bill",
                service_id: "search",
                verified_at_unix_seconds: 50_005,
                timeout: Duration::from_secs(5),
            },
        ))
        .expect_err("tampered restore must fail closed");

    assert!(error.to_string().contains("checksum"));
    assert!(restore_engine.calls().is_empty());
    std::fs::remove_dir_all(root).expect("remove tampered restore fixture");
}

fn fixture() -> (OwnedContainer, OwnedVolume, ResourceRecord) {
    let container_metadata = metadata(ResourceKind::ProjectService, RetentionClass::Disposable)
        .with_resource_id("search")
        .expect("service identity");
    let volume_metadata = metadata(ResourceKind::Volume, RetentionClass::Persistent)
        .with_resource_id("search")
        .expect("volume identity");
    let container = reconstruct_owned_container(
        &ObservedContainer::new(
            ContainerId::new("search-container"),
            container_metadata.labels(),
        ),
        "install-1",
        8,
    )
    .expect("owned container");
    let volume = reconstruct_owned_volume(
        &ObservedVolume::new("stackctl-bill-search-data", volume_metadata.labels()),
        "install-1",
        8,
    )
    .expect("owned volume");
    let resource = ResourceRecord::new(ResourceRecordOptions {
        resource_id: volume.name().to_owned(),
        installation_id: "install-1".to_owned(),
        kind: "volume".to_owned(),
        compatibility_fingerprint: "sha256:search-3".to_owned(),
        project_id: Some("bill".to_owned()),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
    .with_scope_id("search");

    (container, volume, resource)
}

fn metadata(kind: ResourceKind, retention: RetentionClass) -> ManagedResourceMetadata {
    ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind,
        project_id: Some("bill".to_owned()),
        compatibility_fingerprint: "sha256:search-3".to_owned(),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention,
    })
    .expect("managed metadata")
}

fn temporary_directory(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "stackctl-volume-backup-{name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    ))
}

struct RecordingVolumeArchiveEngine {
    archive: Vec<u8>,
    fail_archive: bool,
    calls: Arc<Mutex<Vec<&'static str>>>,
}

impl RecordingVolumeArchiveEngine {
    fn new(archive: Vec<u8>) -> Self {
        Self {
            archive,
            fail_archive: false,
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    const fn with_archive_failure(mut self) -> Self {
        self.fail_archive = true;
        self
    }

    fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().expect("recorded calls").clone()
    }
}

impl ContainerLifecycle for RecordingVolumeArchiveEngine {
    fn create<'operation>(
        &'operation mut self,
        _options: &'operation ContainerCreateOptions,
    ) -> EngineFuture<'operation, OwnedContainer> {
        Box::pin(async {
            Err(EngineError::Backend {
                detail: "unexpected container creation".to_owned(),
            })
        })
    }

    fn start<'operation>(
        &'operation mut self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        self.calls.lock().expect("record start").push("start");
        Box::pin(async { Ok(()) })
    }

    fn stop<'operation>(
        &'operation mut self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        self.calls.lock().expect("record stop").push("stop");
        Box::pin(async { Ok(()) })
    }

    fn remove<'operation>(
        &'operation mut self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }

    fn inspect<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ContainerState> {
        self.calls.lock().expect("record inspect").push("inspect");
        Box::pin(async { Ok(ContainerState::Running) })
    }
}

impl ContainerVolumeArchive for RecordingVolumeArchiveEngine {
    fn download_volume_archive<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
        _volume: &'operation OwnedVolume,
        output: &'operation mut (dyn tokio::io::AsyncWrite + Send + Unpin),
    ) -> EngineFuture<'operation, ()> {
        self.calls.lock().expect("record archive").push("archive");
        let archive = self.archive.clone();
        let fail = self.fail_archive;
        Box::pin(async move {
            if fail {
                return Err(EngineError::Backend {
                    detail: "archive unavailable".to_owned(),
                });
            }
            output
                .write_all(&archive)
                .await
                .map_err(|error| EngineError::Backend {
                    detail: error.to_string(),
                })
        })
    }

    fn upload_volume_archive<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
        _volume: &'operation OwnedVolume,
        _archive: &'operation std::path::Path,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async {
            Err(EngineError::InvalidRequest {
                detail: "upload is outside backup test scope".to_owned(),
            })
        })
    }

    fn download_volume_subpath_archive<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
        _volume: &'operation OwnedVolume,
        _relative_path: &'operation std::path::Path,
        _output: &'operation mut (dyn tokio::io::AsyncWrite + Send + Unpin),
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async {
            Err(EngineError::InvalidRequest {
                detail: "subpath download is outside volume backup test scope".to_owned(),
            })
        })
    }

    fn upload_volume_subpath_archive<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
        _volume: &'operation OwnedVolume,
        _relative_path: &'operation std::path::Path,
        _archive: &'operation std::path::Path,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async {
            Err(EngineError::InvalidRequest {
                detail: "subpath upload is outside volume backup test scope".to_owned(),
            })
        })
    }
}

#[derive(Default)]
struct RecordingVolumeRestoreEngine {
    calls: Arc<Mutex<Vec<&'static str>>>,
    uploaded: Arc<Mutex<Vec<u8>>>,
}

impl RecordingVolumeRestoreEngine {
    fn calls(&self) -> Vec<&'static str> {
        self.calls.lock().expect("restore calls").clone()
    }

    fn uploaded(&self) -> Vec<u8> {
        self.uploaded.lock().expect("uploaded archive").clone()
    }
}

impl ContainerLifecycle for RecordingVolumeRestoreEngine {
    fn create<'operation>(
        &'operation mut self,
        options: &'operation ContainerCreateOptions,
    ) -> EngineFuture<'operation, OwnedContainer> {
        self.calls
            .lock()
            .expect("create container")
            .push("create_container");
        let container = reconstruct_owned_container(
            &ObservedContainer::new(
                ContainerId::new("restored-search-container"),
                options.metadata().labels(),
            ),
            options.metadata().installation_id(),
            options.metadata().schema_version(),
        )
        .map_err(|ownership| EngineError::Backend {
            detail: format!("test container reconstruction failed: {ownership:?}"),
        });
        Box::pin(async move { container })
    }

    fn start<'operation>(
        &'operation mut self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        self.calls.lock().expect("start container").push("start");
        Box::pin(async { Ok(()) })
    }

    fn stop<'operation>(
        &'operation mut self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        self.calls.lock().expect("stop container").push("stop");
        Box::pin(async { Ok(()) })
    }

    fn remove<'operation>(
        &'operation mut self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        self.calls
            .lock()
            .expect("remove container")
            .push("remove_container");
        Box::pin(async { Ok(()) })
    }

    fn inspect<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ContainerState> {
        self.calls
            .lock()
            .expect("inspect container")
            .push("inspect");
        Box::pin(async { Ok(ContainerState::Running) })
    }
}

impl VolumeManager for RecordingVolumeRestoreEngine {
    fn create_volume<'operation>(
        &'operation mut self,
        options: &'operation VolumeCreateOptions,
    ) -> EngineFuture<'operation, OwnedVolume> {
        self.calls
            .lock()
            .expect("create volume")
            .push("create_volume");
        let volume = reconstruct_owned_volume(
            &ObservedVolume::new(options.name(), options.metadata().labels()),
            options.metadata().installation_id(),
            options.metadata().schema_version(),
        )
        .map_err(|ownership| EngineError::Backend {
            detail: format!("test volume reconstruction failed: {ownership:?}"),
        });
        Box::pin(async move { volume })
    }

    fn remove_volume<'operation>(
        &'operation mut self,
        _volume: &'operation OwnedVolume,
    ) -> EngineFuture<'operation, ()> {
        self.calls
            .lock()
            .expect("remove volume")
            .push("remove_volume");
        Box::pin(async { Ok(()) })
    }
}

impl ContainerVolumeArchive for RecordingVolumeRestoreEngine {
    fn download_volume_archive<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
        _volume: &'operation OwnedVolume,
        _output: &'operation mut (dyn tokio::io::AsyncWrite + Send + Unpin),
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async {
            Err(EngineError::InvalidRequest {
                detail: "download is outside restore test scope".to_owned(),
            })
        })
    }

    fn upload_volume_archive<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
        _volume: &'operation OwnedVolume,
        archive: &'operation std::path::Path,
    ) -> EngineFuture<'operation, ()> {
        self.calls.lock().expect("upload archive").push("upload");
        let uploaded = self.uploaded.clone();
        let archive = archive.to_path_buf();
        Box::pin(async move {
            let bytes = tokio::fs::read(archive)
                .await
                .map_err(|error| EngineError::Backend {
                    detail: error.to_string(),
                })?;
            *uploaded.lock().expect("record uploaded archive") = bytes;

            Ok(())
        })
    }

    fn download_volume_subpath_archive<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
        _volume: &'operation OwnedVolume,
        _relative_path: &'operation std::path::Path,
        _output: &'operation mut (dyn tokio::io::AsyncWrite + Send + Unpin),
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async {
            Err(EngineError::InvalidRequest {
                detail: "subpath download is outside volume restore test scope".to_owned(),
            })
        })
    }

    fn upload_volume_subpath_archive<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
        _volume: &'operation OwnedVolume,
        _relative_path: &'operation std::path::Path,
        _archive: &'operation std::path::Path,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async {
            Err(EngineError::InvalidRequest {
                detail: "subpath upload is outside volume restore test scope".to_owned(),
            })
        })
    }
}

impl HealthObserver for RecordingVolumeRestoreEngine {
    fn observe_health<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ContainerHealth> {
        self.calls.lock().expect("observe health").push("health");
        Box::pin(async { Ok(ContainerHealth::Healthy) })
    }
}
