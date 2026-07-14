use super::{ProjectVolumeBackupOptions, backup_project_volume};
use crate::control_plane::engine::{
    ContainerCreateOptions, ContainerId, ContainerLifecycle, ContainerState,
    ContainerVolumeArchive, EngineError, EngineFuture, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ObservedContainer, ObservedVolume, OwnedContainer, OwnedVolume,
    ResourceKind, RetentionClass, reconstruct_owned_container, reconstruct_owned_volume,
};
use crate::control_plane::state::{
    ResourceLifecycle, ResourceRecord, ResourceRecordOptions, ResourceRetention,
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

fn fixture() -> (OwnedContainer, OwnedVolume, ResourceRecord) {
    let container_metadata = metadata(ResourceKind::ProjectService)
        .with_resource_id("search")
        .expect("service identity");
    let volume_metadata = metadata(ResourceKind::Volume)
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

fn metadata(kind: ResourceKind) -> ManagedResourceMetadata {
    ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind,
        project_id: Some("bill".to_owned()),
        compatibility_fingerprint: "sha256:search-3".to_owned(),
        schema_version: 8,
        desired_revision: "sha256:desired".to_owned(),
        retention: RetentionClass::Persistent,
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
}
