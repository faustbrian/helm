use super::{
    ObjectStorePreparationOptions, prepare_object_store_shared_instances,
    reconcile_prepared_object_store_instance,
};
use crate::control_plane::engine::{
    AttachedCommandOptions, BollardEngineAdapter, CommandRequest, ContainerDiscovery, EngineError,
    ImageResolver, ImmutableImageReference, InstallationResourceDeletionOptions,
    ManagedResourceMetadata, ManagedResourceMetadataOptions, NetworkCreateOptions, NetworkManager,
    OwnedContainer, ResourceKind, RetentionClass, delete_owned_installation_resources,
    reconstruct_owned_container, run_attached_command_capture,
};
use crate::control_plane::shared_infrastructure::{
    CompatibilityFingerprintOptions, CompatibilityProfile, CredentialEntropy,
    CredentialGenerationError, IsolationCapability, PersistenceMode, SharedServiceRequest,
    plan_shared_instances,
};
use crate::control_plane::state::SqliteStateStore;
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

async fn write_and_read(
    engine: &BollardEngineAdapter,
    container: &OwnedContainer,
    bucket: &str,
    username: &str,
    password: &str,
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
            format!("tenant/{bucket}/acceptance.txt"),
        ],
        value.as_bytes().to_vec(),
    )
    .await?;
    minio_command(
        engine,
        container,
        username,
        password,
        vec![
            "mc".to_owned(),
            "cat".to_owned(),
            format!("tenant/{bucket}/acceptance.txt"),
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
