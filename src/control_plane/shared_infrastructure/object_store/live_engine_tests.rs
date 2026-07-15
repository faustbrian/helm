use super::{
    ObjectStorePreparationOptions, prepare_object_store_shared_instances,
    reconcile_prepared_object_store_instance,
};
use crate::control_plane::engine::{
    AttachedCommandOptions, BollardEngineAdapter, CommandRequest, ContainerDiscovery, EngineError,
    ImageResolver, ImmutableImageReference, InstallationResourceDeletionOptions,
    ManagedResourceMetadata, ManagedResourceMetadataOptions, NetworkCreateOptions, NetworkManager,
    OwnedContainer, OwnedVolume, ResourceKind, RetentionClass, VolumeDiscovery,
    delete_owned_installation_resources, reconstruct_owned_container, reconstruct_owned_volume,
    run_attached_command_capture,
};
use crate::control_plane::migration::{
    MinioBackupOptions, MinioRestoreOptions, backup_minio_bucket, restore_minio_bucket,
};
use crate::control_plane::shared_infrastructure::{
    CompatibilityFingerprintOptions, CompatibilityProfile, CredentialEntropy,
    CredentialGenerationError, IsolationCapability, OrphanedSharedAccessOptions, PersistenceMode,
    SharedServiceRequest, plan_shared_instances, revoke_orphaned_shared_access_from_observed,
};
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, CredentialRecordOptions, LogicalResourceRecord,
    LogicalResourceRecordOptions, RecoveryPointRecord, RecoveryPointRecordOptions,
    ResourceLifecycle, SqliteStateStore,
};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MINIO_IMAGE: &str = concat!(
    "minio/minio@sha256:",
    "14cea493d9a34af32f524e538b8346cf79f3321eff8e708c1e2960462bd8936e"
);

#[test]
#[ignore = "CI owns live shared MinIO isolation acceptance"]
fn live_docker_engine_two_projects_share_one_minio_with_isolated_buckets() {
    let socket = std::env::var_os("STACKCTL_ENGINE_SOCKET")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/var/run/docker.sock"));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must follow the Unix epoch")
        .as_nanos();
    let installation_id = format!("ci-{}-{nonce}", std::process::id());
    let network_name = format!("stackctl-{installation_id}");
    let state_directory = std::env::temp_dir().join(format!("stackctl-{installation_id}"));
    let platform = match std::env::consts::ARCH {
        "aarch64" => "linux/arm64",
        "x86_64" => "linux/amd64",
        architecture => panic!("unsupported MinIO acceptance architecture '{architecture}'"),
    };
    let profile = CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
        implementation: "minio".to_owned(),
        major_version: "1".to_owned(),
        image_digest: MINIO_IMAGE.to_owned(),
        extensions: Vec::new(),
        immutable_settings: BTreeMap::new(),
        persistence: PersistenceMode::Persistent,
        isolation: IsolationCapability::BucketAndPolicy,
        platform_architecture: Some(platform.to_owned()),
    })
    .expect("build MinIO acceptance profile");
    let shared = plan_shared_instances(vec![
        SharedServiceRequest::new("bill", "storage", profile.clone()),
        SharedServiceRequest::new("shop", "storage", profile),
    ]);
    assert_eq!(shared.len(), 1, "compatible projects must share one plan");
    assert_eq!(shared[0].consumers().len(), 2);
    std::fs::create_dir(&state_directory).expect("create MinIO acceptance state");
    let mut store = SqliteStateStore::open(&state_directory.join("state.sqlite3"))
        .expect("open MinIO acceptance state store");
    let prepared = prepare_object_store_shared_instances(
        &mut store,
        &shared,
        &SequentialCredentialEntropy::new(0x41),
        ObjectStorePreparationOptions {
            installation_id: &installation_id,
            network_name: &network_name,
            schema_version: 8,
            state_directory: &state_directory,
        },
    )
    .expect("prepare shared MinIO acceptance resources");
    let replayed = prepare_object_store_shared_instances(
        &mut store,
        &shared,
        &SequentialCredentialEntropy::new(0x81),
        ObjectStorePreparationOptions {
            installation_id: &installation_id,
            network_name: &network_name,
            schema_version: 8,
            state_directory: &state_directory,
        },
    )
    .expect("replay shared MinIO acceptance preparation");
    assert_eq!(
        prepared[0].instance().root_credential(),
        replayed[0].instance().root_credential()
    );
    let prepared_credentials = prepared[0]
        .projects()
        .iter()
        .map(|project| project.credential().clone())
        .collect::<Vec<_>>();
    let replayed_credentials = replayed[0]
        .projects()
        .iter()
        .map(|project| project.credential().clone())
        .collect::<Vec<_>>();
    assert_eq!(prepared_credentials, replayed_credentials);
    assert_ne!(
        prepared_credentials[0].secret(),
        prepared_credentials[1].secret()
    );
    let prepared = &prepared[0];
    let bill = prepared
        .projects()
        .iter()
        .find(|project| project.environment().project_id() == "bill")
        .expect("prepared bill MinIO resources");
    let shop = prepared
        .projects()
        .iter()
        .find(|project| project.environment().project_id() == "shop")
        .expect("prepared shop MinIO resources");
    let network_metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.clone(),
        kind: ResourceKind::Network,
        project_id: None,
        compatibility_fingerprint: shared[0].fingerprint().as_str().to_owned(),
        schema_version: 8,
        desired_revision: shared[0].fingerprint().as_str().to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("build MinIO acceptance network metadata");
    let network_request = NetworkCreateOptions::new(&network_name, network_metadata)
        .expect("build MinIO acceptance network request");
    let image = ImmutableImageReference::new(MINIO_IMAGE)
        .expect("build immutable MinIO acceptance image reference");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build MinIO acceptance runtime");
    let mut engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    runtime.block_on(async {
        engine
            .ensure_image(&image)
            .await
            .expect("resolve immutable MinIO acceptance image");
        engine
            .create_network(&network_request)
            .await
            .expect("create private MinIO acceptance network");
        let first =
            reconcile_prepared_object_store_instance(&mut engine, prepared, &installation_id, 8)
                .await
                .expect("converge shared MinIO for two projects");
        assert!(
            first.logical_resource_drifts().is_empty(),
            "initial MinIO convergence reported drift: {:?}",
            first.logical_resource_drifts()
        );
        assert_eq!(first.logical_resources().len(), 2);
        let container = owned_shared_container(&engine, &installation_id).await;
        let volume = owned_shared_volume(&engine, &installation_id).await;

        let bill_output = write_and_read(
            &engine,
            &container,
            bill.definition().bucket(),
            bill.credential().username(),
            bill.credential().secret(),
            "bill-value",
        )
        .await
        .expect("write and read bill MinIO bucket");
        assert_eq!(bill_output, b"bill-value");
        let shop_output = write_and_read(
            &engine,
            &container,
            shop.definition().bucket(),
            shop.credential().username(),
            shop.credential().secret(),
            "shop-value",
        )
        .await
        .expect("write and read shop MinIO bucket");
        assert_eq!(shop_output, b"shop-value");
        assert!(matches!(
            minio_command(
                &engine,
                &container,
                shop.credential().username(),
                shop.credential().secret(),
                vec![
                    "mc".to_owned(),
                    "cat".to_owned(),
                    format!("tenant/{}/acceptance.txt", bill.definition().bucket()),
                ],
                Vec::new(),
            )
            .await,
            Err(EngineError::ContainerExit { .. })
        ));

        let second =
            reconcile_prepared_object_store_instance(&mut engine, prepared, &installation_id, 8)
                .await
                .expect("reconcile unchanged shared MinIO instance");
        assert!(second.logical_resource_drifts().is_empty());
        let replayed_container = owned_shared_container(&engine, &installation_id).await;
        assert_eq!(replayed_container.id(), container.id());

        let bill_logical = first
            .logical_resources()
            .iter()
            .find(|logical| logical.project_id() == "bill")
            .expect("find active bill MinIO resource");
        let backup = backup_minio_bucket(
            &engine,
            &container,
            &volume,
            &MinioBackupOptions {
                logical_resource: bill_logical,
                credential: bill.credential(),
                installation_id: &installation_id,
                created_at_unix_seconds: 10_000,
                backup_root: &state_directory.join("backups"),
                timeout: Duration::from_secs(30),
            },
        )
        .await
        .expect("back up bill MinIO bucket");
        write_object(
            &engine,
            &container,
            bill.definition().bucket(),
            bill.credential().username(),
            bill.credential().secret(),
            "acceptance.txt",
            "bill-mutated-after-backup",
        )
        .await
        .expect("mutate bill object after backup");
        write_object(
            &engine,
            &container,
            bill.definition().bucket(),
            bill.credential().username(),
            bill.credential().secret(),
            "post-backup.txt",
            "must-be-removed",
        )
        .await
        .expect("create bill object after backup");
        let recovery = RecoveryPointRecord::new(RecoveryPointRecordOptions {
            recovery_point_id: "bill-minio-live-recovery".to_owned(),
            project_id: bill_logical.project_id().to_owned(),
            service_id: bill_logical.service_id().to_owned(),
            logical_resource_id: bill_logical.logical_resource_id().to_owned(),
            resource_kind: bill_logical.kind().to_owned(),
            compatibility_fingerprint: bill_logical.compatibility_fingerprint().to_owned(),
            reference: backup.reference().to_owned(),
            artifact_sha256: backup.artifact_sha256().to_owned(),
            artifact_size_bytes: backup.artifact_size_bytes(),
            created_at_unix_seconds: 10_000,
            verified_at_unix_seconds: 10_000,
        })
        .expect("record bill MinIO recovery point");
        restore_minio_bucket(
            &engine,
            &container,
            &volume,
            &MinioRestoreOptions {
                recovery_point: &recovery,
                logical_resource: bill_logical,
                credential: bill.credential(),
                installation_id: &installation_id,
                target_bucket_name: bill.definition().bucket(),
                verified_at_unix_seconds: 10_001,
                timeout: Duration::from_secs(30),
            },
        )
        .await
        .expect("restore bill MinIO bucket");
        let restored_bill = read_object(
            &engine,
            &container,
            bill.definition().bucket(),
            bill.credential().username(),
            bill.credential().secret(),
            "acceptance.txt",
        )
        .await
        .expect("read restored bill object");
        assert_eq!(restored_bill, b"bill-value");
        assert!(matches!(
            read_object(
                &engine,
                &container,
                bill.definition().bucket(),
                bill.credential().username(),
                bill.credential().secret(),
                "post-backup.txt",
            )
            .await,
            Err(EngineError::ContainerExit { .. })
        ));
        let preserved_shop = read_object(
            &engine,
            &container,
            shop.definition().bucket(),
            shop.credential().username(),
            shop.credential().secret(),
            "acceptance.txt",
        )
        .await
        .expect("read preserved shop object");
        assert_eq!(preserved_shop, b"shop-value");

        let bill_logical = first
            .logical_resources()
            .iter()
            .find(|logical| logical.project_id() == "bill")
            .map(orphaned_logical_resource)
            .expect("find bill logical MinIO resource");
        let credentials = [
            disabled_credential(bill.credential()),
            prepared.instance().root_credential().clone(),
        ];
        let observed = engine
            .discover_managed()
            .await
            .expect("discover MinIO lifecycle acceptance resources");
        let revoked = revoke_orphaned_shared_access_from_observed(
            &mut engine,
            &observed,
            OrphanedSharedAccessOptions {
                resources: first.physical_resources(),
                logical_resources: std::slice::from_ref(&bill_logical),
                credentials: &credentials,
                installation_id: &installation_id,
                schema_version: 8,
                timeout: Duration::from_secs(15),
            },
        )
        .await
        .expect("revoke removed bill MinIO access");
        assert_eq!(revoked, 1);
        assert!(matches!(
            minio_command(
                &engine,
                &container,
                bill.credential().username(),
                bill.credential().secret(),
                vec![
                    "mc".to_owned(),
                    "cat".to_owned(),
                    format!("tenant/{}/acceptance.txt", bill.definition().bucket()),
                ],
                Vec::new(),
            )
            .await,
            Err(EngineError::ContainerExit { .. })
        ));
        let retained_shop_object = minio_command(
            &engine,
            &container,
            shop.credential().username(),
            shop.credential().secret(),
            vec![
                "mc".to_owned(),
                "cat".to_owned(),
                format!("tenant/{}/acceptance.txt", shop.definition().bucket()),
            ],
            Vec::new(),
        )
        .await
        .expect("read shop object after bill removal");
        assert_eq!(retained_shop_object, b"shop-value");
        let retained_bill_object = minio_command(
            &engine,
            &container,
            prepared.instance().root_credential().username(),
            prepared.instance().root_credential().secret(),
            vec![
                "mc".to_owned(),
                "cat".to_owned(),
                format!("tenant/{}/acceptance.txt", bill.definition().bucket()),
            ],
            Vec::new(),
        )
        .await
        .expect("inspect retained bill object after identity revocation");
        assert_eq!(retained_bill_object, b"bill-value");

        let restored =
            reconcile_prepared_object_store_instance(&mut engine, prepared, &installation_id, 8)
                .await
                .expect("restore bill MinIO access from active configuration");
        assert!(restored.logical_resource_drifts().is_empty());
        let restored_bill_object = minio_command(
            &engine,
            &container,
            bill.credential().username(),
            bill.credential().secret(),
            vec![
                "mc".to_owned(),
                "cat".to_owned(),
                format!("tenant/{}/acceptance.txt", bill.definition().bucket()),
            ],
            Vec::new(),
        )
        .await
        .expect("read bill object after access restoration");
        assert_eq!(restored_bill_object, b"bill-value");
        let restored_container = owned_shared_container(&engine, &installation_id).await;
        assert_eq!(restored_container.id(), container.id());

        delete_owned_installation_resources(
            &mut engine,
            InstallationResourceDeletionOptions {
                installation_id: &installation_id,
                schema_version: 8,
                authorized_persistent_volumes: &[prepared
                    .instance()
                    .volume()
                    .expect("persistent MinIO volume")
                    .name()
                    .to_owned()],
            },
        )
        .await
        .expect("delete shared MinIO acceptance resources");
    });

    drop(store);
    std::fs::remove_dir_all(&state_directory).expect("remove MinIO acceptance state");
    println!("shared MinIO isolation acceptance passed for {installation_id}");
}

fn orphaned_logical_resource(logical: &LogicalResourceRecord) -> LogicalResourceRecord {
    LogicalResourceRecord::new(LogicalResourceRecordOptions {
        logical_resource_id: logical.logical_resource_id().to_owned(),
        shared_resource_id: logical.shared_resource_id().to_owned(),
        project_id: logical.project_id().to_owned(),
        service_id: logical.service_id().to_owned(),
        kind: logical.kind().to_owned(),
        compatibility_fingerprint: logical.compatibility_fingerprint().to_owned(),
        desired_revision: logical.desired_revision().to_owned(),
        lifecycle: ResourceLifecycle::Orphaned,
        orphaned_at_unix_seconds: Some(12_345),
    })
}

fn disabled_credential(credential: &CredentialRecord) -> CredentialRecord {
    CredentialRecord::new(CredentialRecordOptions {
        credential_id: credential.credential_id().to_owned(),
        project_id: credential.project_id().map(str::to_owned),
        service_id: credential.service_id().to_owned(),
        username: credential.username().to_owned(),
        secret: credential.secret().to_owned(),
        lifecycle: CredentialLifecycle::Disabled,
    })
}

async fn owned_shared_container(
    engine: &BollardEngineAdapter,
    installation_id: &str,
) -> OwnedContainer {
    engine
        .discover_managed()
        .await
        .expect("discover shared MinIO acceptance resources")
        .into_iter()
        .filter_map(|observed| reconstruct_owned_container(&observed, installation_id, 8).ok())
        .find(|owned| owned.metadata().kind() == ResourceKind::SharedService)
        .expect("discover one owned shared MinIO container")
}

async fn owned_shared_volume(engine: &BollardEngineAdapter, installation_id: &str) -> OwnedVolume {
    engine
        .discover_managed_volumes()
        .await
        .expect("discover shared MinIO acceptance volumes")
        .into_iter()
        .filter_map(|observed| reconstruct_owned_volume(&observed, installation_id, 8).ok())
        .find(|owned| owned.metadata().kind() == ResourceKind::Volume)
        .expect("discover one owned shared MinIO volume")
}

async fn write_and_read(
    engine: &BollardEngineAdapter,
    container: &OwnedContainer,
    bucket: &str,
    username: &str,
    password: &str,
    value: &str,
) -> Result<Vec<u8>, EngineError> {
    write_object(
        engine,
        container,
        bucket,
        username,
        password,
        "acceptance.txt",
        value,
    )
    .await?;
    read_object(
        engine,
        container,
        bucket,
        username,
        password,
        "acceptance.txt",
    )
    .await
}

async fn write_object(
    engine: &BollardEngineAdapter,
    container: &OwnedContainer,
    bucket: &str,
    username: &str,
    password: &str,
    object: &str,
    value: &str,
) -> Result<Vec<u8>, EngineError> {
    minio_command(
        engine,
        container,
        username,
        password,
        vec![
            "mc".to_owned(),
            "pipe".to_owned(),
            format!("tenant/{bucket}/{object}"),
        ],
        value.as_bytes().to_vec(),
    )
    .await
}

async fn read_object(
    engine: &BollardEngineAdapter,
    container: &OwnedContainer,
    bucket: &str,
    username: &str,
    password: &str,
    object: &str,
) -> Result<Vec<u8>, EngineError> {
    minio_command(
        engine,
        container,
        username,
        password,
        vec![
            "mc".to_owned(),
            "cat".to_owned(),
            format!("tenant/{bucket}/{object}"),
        ],
        Vec::new(),
    )
    .await
}

async fn minio_command(
    engine: &BollardEngineAdapter,
    container: &OwnedContainer,
    username: &str,
    password: &str,
    arguments: Vec<String>,
    input: Vec<u8>,
) -> Result<Vec<u8>, EngineError> {
    let request = CommandRequest::new(
        arguments,
        BTreeMap::from([(
            "MC_HOST_tenant".to_owned(),
            format!("http://{username}:{password}@127.0.0.1:9000"),
        )]),
        None,
    )?;
    let options = AttachedCommandOptions::new(
        request,
        input,
        "exercise MinIO tenant isolation",
        Duration::from_secs(15),
    )?;

    run_attached_command_capture(engine, container, &options).await
}

struct SequentialCredentialEntropy {
    next: AtomicU8,
}

impl SequentialCredentialEntropy {
    const fn new(first: u8) -> Self {
        Self {
            next: AtomicU8::new(first),
        }
    }
}

impl CredentialEntropy for SequentialCredentialEntropy {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), CredentialGenerationError> {
        bytes.fill(self.next.fetch_add(1, Ordering::SeqCst));

        Ok(())
    }
}
