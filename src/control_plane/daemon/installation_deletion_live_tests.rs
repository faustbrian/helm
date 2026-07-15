use super::finalize_installation_deletion;
use crate::control_plane::application::ControlPlane;
use crate::control_plane::engine::{
    BollardEngineAdapter, ManagedResourceMetadata, ManagedResourceMetadataOptions, ResourceKind,
    RetentionClass, VolumeCreateOptions, VolumeDiscovery, VolumeManager,
};
use crate::control_plane::retention::{BackupResourceIdentity, store_backup_artifact_for_identity};
use crate::control_plane::state::{
    EngineProvider, InstallationLifecycle, InstallationRecord, RecoveryPointRecord,
    RecoveryPointRecordOptions, ResourceLifecycle, ResourceRecord, ResourceRecordOptions,
    ResourceRetention, SqliteStateStore, StateStore,
};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
#[ignore = "CI owns live confirmed installation deletion acceptance"]
fn live_docker_engine_confirmed_installation_deletion_uses_recovery_authorized_volume() {
    let socket = std::env::var_os("STACKCTL_ENGINE_SOCKET")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/var/run/docker.sock"));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must follow the Unix epoch")
        .as_nanos();
    let installation_id = format!("ci-{}-{nonce}", std::process::id());
    let volume_name = format!("stackctl-{installation_id}-project-data");
    let state_directory = std::env::temp_dir().join(format!("stackctl-{installation_id}"));
    std::fs::create_dir(&state_directory).expect("create deletion acceptance state");
    let resource = ResourceRecord::new(ResourceRecordOptions {
        resource_id: volume_name.clone(),
        installation_id: installation_id.clone(),
        kind: "volume".to_owned(),
        compatibility_fingerprint: format!("sha256:{}", "a".repeat(64)),
        project_id: Some("ci-project".to_owned()),
        schema_version: 8,
        desired_revision: format!("sha256:{}", "b".repeat(64)),
        retention: ResourceRetention::Persistent,
        lifecycle: ResourceLifecycle::Active,
        orphaned_at_unix_seconds: None,
    })
    .with_scope_id("data");
    let stored = store_backup_artifact_for_identity(
        &BackupResourceIdentity::from_resource(&resource),
        b"volume archive",
        40_000,
        &state_directory.join("backups"),
    )
    .expect("store deletion acceptance recovery artifact");
    let recovery = RecoveryPointRecord::new(RecoveryPointRecordOptions {
        recovery_point_id: "volume-backup-42".to_owned(),
        project_id: "ci-project".to_owned(),
        service_id: "data".to_owned(),
        logical_resource_id: resource.resource_id().to_owned(),
        resource_kind: resource.kind().to_owned(),
        compatibility_fingerprint: resource.compatibility_fingerprint().to_owned(),
        reference: stored.recovery_point().display().to_string(),
        artifact_sha256: hex::encode(Sha256::digest(b"volume archive")),
        artifact_size_bytes: 14,
        created_at_unix_seconds: 40_000,
        verified_at_unix_seconds: 40_000,
    })
    .expect("build deletion acceptance recovery record");
    let mut store = SqliteStateStore::open(&state_directory.join("state.sqlite3"))
        .expect("open deletion acceptance state");
    store
        .initialize_installation(&InstallationRecord::new(
            &installation_id,
            EngineProvider::Docker,
            socket.to_string_lossy(),
        ))
        .expect("initialize deletion acceptance installation");
    store
        .upsert_resources(std::slice::from_ref(&resource))
        .expect("persist deletion acceptance volume");
    store
        .record_recovery_point(&recovery)
        .expect("catalog deletion acceptance recovery point");
    let mut control_plane = ControlPlane::new(store);
    let plan = control_plane
        .plan_installation_deletion()
        .expect("plan confirmed installation deletion");
    assert_eq!(plan.volume_deletions().len(), 1);
    assert_eq!(
        plan.volume_deletions()[0].resource_id(),
        resource.resource_id()
    );
    assert_eq!(
        plan.volume_deletions()[0].recovery_point_id(),
        recovery.recovery_point_id()
    );
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.clone(),
        kind: ResourceKind::Volume,
        project_id: Some("ci-project".to_owned()),
        compatibility_fingerprint: resource.compatibility_fingerprint().to_owned(),
        schema_version: 8,
        desired_revision: resource.desired_revision().to_owned(),
        retention: RetentionClass::Persistent,
    })
    .and_then(|metadata| metadata.with_resource_id("data"))
    .expect("build deletion acceptance volume metadata");
    let volume = VolumeCreateOptions::new(&volume_name, metadata)
        .expect("build deletion acceptance volume request");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build deletion acceptance runtime");
    let mut engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    runtime.block_on(async {
        engine
            .create_volume(&volume)
            .await
            .expect("create deletion acceptance volume");
        assert!(
            !finalize_installation_deletion(&mut control_plane, &mut engine, 8, 40_001)
                .await
                .expect("unconfirmed deletion must remain inactive")
        );
        assert!(volume_exists(&engine, &volume_name).await);

        control_plane
            .begin_confirmed_installation_deletion(plan.confirmation_token(), 40_002)
            .expect("confirm and freeze installation deletion");
        assert!(
            finalize_installation_deletion(&mut control_plane, &mut engine, 8, 40_003)
                .await
                .expect("finalize confirmed installation deletion")
        );
        assert!(!volume_exists(&engine, &volume_name).await);
    });
    assert_eq!(
        control_plane
            .installation_lifecycle()
            .expect("load terminal installation lifecycle"),
        Some(InstallationLifecycle::Deleted)
    );

    drop(control_plane);
    std::fs::remove_dir_all(&state_directory).expect("remove deletion acceptance state");
    println!("confirmed installation deletion passed for {installation_id}");
}

async fn volume_exists(engine: &BollardEngineAdapter, volume_name: &str) -> bool {
    engine
        .discover_managed_volumes()
        .await
        .expect("discover deletion acceptance volumes")
        .iter()
        .any(|volume| volume.name() == volume_name)
}
