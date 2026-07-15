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
const MARIADB_IMAGE: &str = concat!(
    "mariadb@sha256:",
    "efb4959ef2c835cd735dbc388eb9ad6aab0c78dd64febcd51bc17481111890c4"
);

#[test]
#[ignore = "CI owns live shared MySQL isolation acceptance"]
fn live_docker_engine_two_projects_share_one_mysql_with_isolated_schemas() {
    run_live_engine_shared_database_isolation(LiveEngineDatabaseOptions {
        implementation: "mysql",
        display_name: "MySQL",
        major_version: "8",
        image: MYSQL_IMAGE,
        client_executable: "mysql",
    });
}

#[test]
#[ignore = "CI owns live shared MariaDB isolation acceptance"]
fn live_docker_engine_two_projects_share_one_mariadb_with_isolated_schemas() {
    run_live_engine_shared_database_isolation(LiveEngineDatabaseOptions {
        implementation: "mariadb",
        display_name: "MariaDB",
        major_version: "11",
        image: MARIADB_IMAGE,
        client_executable: "mariadb",
    });
}

fn run_live_engine_shared_database_isolation(options: LiveEngineDatabaseOptions<'_>) {
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
        architecture => panic!(
            "unsupported {} acceptance architecture '{architecture}'",
            options.display_name
        ),
    };
    let profile = CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
        implementation: options.implementation.to_owned(),
        major_version: options.major_version.to_owned(),
        image_digest: options.image.to_owned(),
        extensions: Vec::new(),
        immutable_settings: BTreeMap::new(),
        persistence: PersistenceMode::Persistent,
        isolation: IsolationCapability::DatabaseAndRole,
        platform_architecture: Some(platform.to_owned()),
    })
    .unwrap_or_else(|error| panic!("build {} acceptance profile: {error}", options.display_name));
    let shared = plan_shared_instances(vec![
        SharedServiceRequest::new("bill", "database", profile.clone()),
        SharedServiceRequest::new("shop", "database", profile),
    ]);
    assert_eq!(shared.len(), 1, "compatible projects must share one plan");
    assert_eq!(shared[0].consumers().len(), 2);
    std::fs::create_dir(&state_directory).unwrap_or_else(|error| {
        panic!("create {} acceptance state: {error}", options.display_name)
    });
    let mut store =
        SqliteStateStore::open(&state_directory.join("state.sqlite3")).unwrap_or_else(|error| {
            panic!(
                "open {} acceptance state store: {error}",
                options.display_name
            )
        });
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
    .unwrap_or_else(|error| {
        panic!(
            "prepare shared {} acceptance resources: {error}",
            options.display_name
        )
    });
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
    .unwrap_or_else(|error| {
        panic!(
            "replay shared {} acceptance preparation: {error}",
            options.display_name
        )
    });
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
        .unwrap_or_else(|| panic!("prepared bill {} resources", options.display_name));
    let shop = prepared
        .projects()
        .iter()
        .find(|project| project.environment().project_id() == "shop")
        .unwrap_or_else(|| panic!("prepared shop {} resources", options.display_name));
    let network_metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.clone(),
        kind: ResourceKind::Network,
        project_id: None,
        compatibility_fingerprint: shared[0].fingerprint().as_str().to_owned(),
        schema_version: 8,
        desired_revision: shared[0].fingerprint().as_str().to_owned(),
        retention: RetentionClass::Persistent,
    })
    .unwrap_or_else(|error| {
        panic!(
            "build {} acceptance network metadata: {error}",
            options.display_name
        )
    });
    let network_request = NetworkCreateOptions::new(&network_name, network_metadata)
        .unwrap_or_else(|error| {
            panic!(
                "build {} acceptance network request: {error}",
                options.display_name
            )
        });
    let image = ImmutableImageReference::new(options.image).unwrap_or_else(|error| {
        panic!(
            "build immutable {} acceptance image reference: {error}",
            options.display_name
        )
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build MySQL acceptance runtime");
    let mut engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    runtime.block_on(async {
        engine.ensure_image(&image).await.unwrap_or_else(|error| {
            panic!(
                "resolve immutable {} acceptance image: {error}",
                options.display_name
            )
        });
        engine
            .create_network(&network_request)
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "create private {} acceptance network: {error}",
                    options.display_name
                )
            });
        let first = reconcile_prepared_mysql_instance(&mut engine, prepared, &installation_id, 8)
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "converge shared {} for two projects: {error}",
                    options.display_name
                )
            });
        assert!(
            first.logical_resource_drifts().is_empty(),
            "initial {} convergence reported drift: {:?}",
            options.display_name,
            first.logical_resource_drifts()
        );
        assert_eq!(first.logical_resources().len(), 2);
        let container = owned_shared_container(&engine, &installation_id).await;

        let bill_output = database_command(
            &engine,
            &container,
            options.client_executable,
            bill.credential().username(),
            bill.credential().secret(),
            bill.logical().schema_name(),
            "CREATE TABLE IF NOT EXISTS stackctl_acceptance(value varchar(64)); \
             TRUNCATE stackctl_acceptance; \
             INSERT INTO stackctl_acceptance VALUES ('bill-value'); \
             SELECT value FROM stackctl_acceptance;",
        )
        .await
        .unwrap_or_else(|error| {
            panic!(
                "write and read bill {} schema: {error}",
                options.display_name
            )
        });
        assert_eq!(bill_output, b"bill-value\n");
        let shop_output = database_command(
            &engine,
            &container,
            options.client_executable,
            shop.credential().username(),
            shop.credential().secret(),
            shop.logical().schema_name(),
            "CREATE TABLE IF NOT EXISTS stackctl_acceptance(value varchar(64)); \
             TRUNCATE stackctl_acceptance; \
             INSERT INTO stackctl_acceptance VALUES ('shop-value'); \
             SELECT value FROM stackctl_acceptance;",
        )
        .await
        .unwrap_or_else(|error| {
            panic!(
                "write and read shop {} schema: {error}",
                options.display_name
            )
        });
        assert_eq!(shop_output, b"shop-value\n");
        assert!(matches!(
            database_command(
                &engine,
                &container,
                options.client_executable,
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
            .unwrap_or_else(|error| {
                panic!(
                    "reconcile unchanged shared {} instance: {error}",
                    options.display_name
                )
            });
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
                    .unwrap_or_else(|| panic!("persistent {} volume", options.display_name))
                    .name()
                    .to_owned()],
            },
        )
        .await
        .unwrap_or_else(|error| {
            panic!(
                "delete shared {} acceptance resources: {error}",
                options.display_name
            )
        });
    });

    drop(store);
    std::fs::remove_dir_all(&state_directory).unwrap_or_else(|error| {
        panic!("remove {} acceptance state: {error}", options.display_name)
    });
    println!(
        "shared {} isolation acceptance passed for {installation_id}",
        options.display_name
    );
}

async fn owned_shared_container(
    engine: &BollardEngineAdapter,
    installation_id: &str,
) -> OwnedContainer {
    engine
        .discover_managed()
        .await
        .expect("discover shared database acceptance resources")
        .into_iter()
        .filter_map(|observed| reconstruct_owned_container(&observed, installation_id, 8).ok())
        .find(|owned| owned.metadata().kind() == ResourceKind::SharedService)
        .expect("discover one owned shared database container")
}

async fn database_command(
    engine: &BollardEngineAdapter,
    container: &OwnedContainer,
    client_executable: &str,
    username: &str,
    password: &str,
    schema: &str,
    sql: &str,
) -> Result<Vec<u8>, EngineError> {
    let request = CommandRequest::new(
        vec![
            client_executable.to_owned(),
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
        "exercise MySQL-family tenant isolation",
        Duration::from_secs(15),
    )?;

    run_attached_command_capture(engine, container, &options).await
}

#[derive(Clone, Copy)]
struct LiveEngineDatabaseOptions<'a> {
    implementation: &'a str,
    display_name: &'a str,
    major_version: &'a str,
    image: &'a str,
    client_executable: &'a str,
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
