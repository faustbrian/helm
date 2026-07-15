use super::{
    MySqlPreparationOptions, prepare_mysql_shared_instances, reconcile_prepared_mysql_instance,
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

const MYSQL_IMAGE: &str = concat!(
    "mysql@sha256:",
    "c831a0f11348d402b43d77453e17d770be2eef356615a2823fe0f5a0d6c8b9af"
);

#[test]
#[ignore = "CI owns live shared MySQL isolation acceptance"]
fn live_docker_engine_two_projects_share_one_mysql_with_isolated_schemas() {
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
        architecture => panic!("unsupported MySQL acceptance architecture '{architecture}'"),
    };
    let profile = CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
        implementation: "mysql".to_owned(),
        major_version: "8".to_owned(),
        image_digest: MYSQL_IMAGE.to_owned(),
        extensions: Vec::new(),
        immutable_settings: BTreeMap::new(),
        persistence: PersistenceMode::Persistent,
        isolation: IsolationCapability::DatabaseAndRole,
        platform_architecture: Some(platform.to_owned()),
    })
    .expect("build MySQL acceptance profile");
    let shared = plan_shared_instances(vec![
        SharedServiceRequest::new("bill", "database", profile.clone()),
        SharedServiceRequest::new("shop", "database", profile),
    ]);
    assert_eq!(shared.len(), 1, "compatible projects must share one plan");
    assert_eq!(shared[0].consumers().len(), 2);
    std::fs::create_dir(&state_directory).expect("create MySQL acceptance state");
    let mut store = SqliteStateStore::open(&state_directory.join("state.sqlite3"))
        .expect("open MySQL acceptance state store");
    let prepared = prepare_mysql_shared_instances(
        &mut store,
        &shared,
        &SequentialCredentialEntropy::new(0x31),
        MySqlPreparationOptions {
            installation_id: &installation_id,
            network_name: &network_name,
            schema_version: 8,
        },
    )
    .expect("prepare shared MySQL acceptance resources");
    let replayed = prepare_mysql_shared_instances(
        &mut store,
        &shared,
        &SequentialCredentialEntropy::new(0x71),
        MySqlPreparationOptions {
            installation_id: &installation_id,
            network_name: &network_name,
            schema_version: 8,
        },
    )
    .expect("replay shared MySQL acceptance preparation");
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
        .expect("prepared bill MySQL resources");
    let shop = prepared
        .projects()
        .iter()
        .find(|project| project.environment().project_id() == "shop")
        .expect("prepared shop MySQL resources");
    let network_metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.clone(),
        kind: ResourceKind::Network,
        project_id: None,
        compatibility_fingerprint: shared[0].fingerprint().as_str().to_owned(),
        schema_version: 8,
        desired_revision: shared[0].fingerprint().as_str().to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("build MySQL acceptance network metadata");
    let network_request = NetworkCreateOptions::new(&network_name, network_metadata)
        .expect("build MySQL acceptance network request");
    let image = ImmutableImageReference::new(MYSQL_IMAGE)
        .expect("build immutable MySQL acceptance image reference");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build MySQL acceptance runtime");
    let mut engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    runtime.block_on(async {
        engine
            .ensure_image(&image)
            .await
            .expect("resolve immutable MySQL acceptance image");
        engine
            .create_network(&network_request)
            .await
            .expect("create private MySQL acceptance network");
        let first = reconcile_prepared_mysql_instance(&mut engine, prepared, &installation_id, 8)
            .await
            .expect("converge shared MySQL for two projects");
        assert!(
            first.logical_resource_drifts().is_empty(),
            "initial MySQL convergence reported drift: {:?}",
            first.logical_resource_drifts()
        );
        assert_eq!(first.logical_resources().len(), 2);
        let container = owned_shared_container(&engine, &installation_id).await;

        let bill_output = mysql_command(
            &engine,
            &container,
            bill.credential().username(),
            bill.credential().secret(),
            bill.logical().schema_name(),
            "CREATE TABLE IF NOT EXISTS stackctl_acceptance(value varchar(64)); \
             TRUNCATE stackctl_acceptance; \
             INSERT INTO stackctl_acceptance VALUES ('bill-value'); \
             SELECT value FROM stackctl_acceptance;",
        )
        .await
        .expect("write and read bill MySQL schema");
        assert_eq!(bill_output, b"bill-value\n");
        let shop_output = mysql_command(
            &engine,
            &container,
            shop.credential().username(),
            shop.credential().secret(),
            shop.logical().schema_name(),
            "CREATE TABLE IF NOT EXISTS stackctl_acceptance(value varchar(64)); \
             TRUNCATE stackctl_acceptance; \
             INSERT INTO stackctl_acceptance VALUES ('shop-value'); \
             SELECT value FROM stackctl_acceptance;",
        )
        .await
        .expect("write and read shop MySQL schema");
        assert_eq!(shop_output, b"shop-value\n");
        assert!(matches!(
            mysql_command(
                &engine,
                &container,
                shop.credential().username(),
                shop.credential().secret(),
                bill.logical().schema_name(),
                "SELECT 1;",
            )
            .await,
            Err(EngineError::ContainerExit { .. })
        ));

        let second = reconcile_prepared_mysql_instance(&mut engine, prepared, &installation_id, 8)
            .await
            .expect("reconcile unchanged shared MySQL instance");
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
                    .expect("persistent MySQL volume")
                    .name()
                    .to_owned()],
            },
        )
        .await
        .expect("delete shared MySQL acceptance resources");
    });

    drop(store);
    std::fs::remove_dir_all(&state_directory).expect("remove MySQL acceptance state");
    println!("shared MySQL isolation acceptance passed for {installation_id}");
}

async fn owned_shared_container(
    engine: &BollardEngineAdapter,
    installation_id: &str,
) -> OwnedContainer {
    engine
        .discover_managed()
        .await
        .expect("discover shared MySQL acceptance resources")
        .into_iter()
        .filter_map(|observed| reconstruct_owned_container(&observed, installation_id, 8).ok())
        .find(|owned| owned.metadata().kind() == ResourceKind::SharedService)
        .expect("discover one owned shared MySQL container")
}

async fn mysql_command(
    engine: &BollardEngineAdapter,
    container: &OwnedContainer,
    username: &str,
    password: &str,
    schema: &str,
    sql: &str,
) -> Result<Vec<u8>, EngineError> {
    let request = CommandRequest::new(
        vec![
            "mysql".to_owned(),
            "--protocol=socket".to_owned(),
            format!("--user={username}"),
            "--batch".to_owned(),
            "--skip-column-names".to_owned(),
            format!("--database={schema}"),
            format!("--execute={sql}"),
        ],
        BTreeMap::from([("MYSQL_PWD".to_owned(), password.to_owned())]),
        None,
    )?;
    let options = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "exercise MySQL tenant isolation",
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
