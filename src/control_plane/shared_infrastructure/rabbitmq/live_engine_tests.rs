use super::{
    RabbitMqPreparationOptions, prepare_rabbitmq_shared_instances,
    reconcile_prepared_rabbitmq_instance,
};
use crate::control_plane::engine::{
    AttachedCommandOptions, BollardEngineAdapter, CommandRequest, ContainerDiscovery, EngineError,
    ImageResolver, ImmutableImageReference, InstallationResourceDeletionOptions, NetworkManager,
    OwnedContainer, ResourceKind, delete_owned_installation_resources, reconstruct_owned_container,
    run_attached_command_capture,
};
use crate::control_plane::network::global_network_request;
use crate::control_plane::shared_infrastructure::{
    CompatibilityFingerprintOptions, CompatibilityProfile, CredentialEntropy,
    CredentialGenerationError, IsolationCapability, PersistenceMode, SharedServiceRequest,
    plan_shared_instances,
};
use crate::control_plane::state::SqliteStateStore;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const RABBITMQ_IMAGE: &str = concat!(
    "rabbitmq@sha256:",
    "d5011b2fee8048d33b7fa4e2d1696020a00ff8d71cecbd164cac9a629ab4edde"
);

#[test]
#[ignore = "CI owns live shared RabbitMQ isolation acceptance"]
fn live_docker_engine_two_projects_share_one_rabbitmq_with_isolated_vhosts() {
    let socket = std::env::var_os("STACKCTL_ENGINE_SOCKET")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/var/run/docker.sock"));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must follow the Unix epoch")
        .as_nanos();
    let installation_id = format!("ci-{}-{nonce}", std::process::id());
    let network_request =
        global_network_request(&installation_id).expect("build RabbitMQ acceptance network");
    let state_directory = std::env::temp_dir().join(format!("stackctl-{installation_id}"));
    let platform = match std::env::consts::ARCH {
        "aarch64" => "linux/arm64",
        "x86_64" => "linux/amd64",
        architecture => panic!("unsupported RabbitMQ acceptance architecture '{architecture}'"),
    };
    let profile = CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
        implementation: "rabbitmq".to_owned(),
        major_version: "4".to_owned(),
        image_digest: RABBITMQ_IMAGE.to_owned(),
        extensions: Vec::new(),
        immutable_settings: BTreeMap::new(),
        persistence: PersistenceMode::Persistent,
        isolation: IsolationCapability::VirtualHostAndUser,
        platform_architecture: Some(platform.to_owned()),
    })
    .expect("build RabbitMQ acceptance profile");
    let shared = plan_shared_instances(vec![
        SharedServiceRequest::new("bill", "broker", profile.clone()),
        SharedServiceRequest::new("shop", "broker", profile),
    ]);
    assert_eq!(shared.len(), 1, "compatible projects must share one plan");
    assert_eq!(shared[0].consumers().len(), 2);
    std::fs::create_dir(&state_directory).expect("create RabbitMQ acceptance state");
    let mut store = SqliteStateStore::open(&state_directory.join("state.sqlite3"))
        .expect("open RabbitMQ acceptance state store");
    let prepared = prepare_rabbitmq_shared_instances(
        &mut store,
        &shared,
        &SequentialCredentialEntropy::new(0x41),
        RabbitMqPreparationOptions {
            installation_id: &installation_id,
            network_name: network_request.name(),
            schema_version: 8,
            state_directory: &state_directory,
        },
    )
    .expect("prepare shared RabbitMQ acceptance resources");
    let replayed = prepare_rabbitmq_shared_instances(
        &mut store,
        &shared,
        &SequentialCredentialEntropy::new(0x81),
        RabbitMqPreparationOptions {
            installation_id: &installation_id,
            network_name: network_request.name(),
            schema_version: 8,
            state_directory: &state_directory,
        },
    )
    .expect("replay shared RabbitMQ acceptance preparation");
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
        .expect("prepared bill RabbitMQ resources");
    let shop = prepared
        .projects()
        .iter()
        .find(|project| project.environment().project_id() == "shop")
        .expect("prepared shop RabbitMQ resources");
    let image = ImmutableImageReference::new(RABBITMQ_IMAGE)
        .expect("build immutable RabbitMQ acceptance image reference");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build RabbitMQ acceptance runtime");
    let mut engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    runtime.block_on(async {
        engine
            .ensure_image(&image)
            .await
            .expect("resolve immutable RabbitMQ acceptance image");
        engine
            .create_network(&network_request)
            .await
            .expect("create private RabbitMQ acceptance network");
        let first =
            reconcile_prepared_rabbitmq_instance(&mut engine, prepared, &installation_id, 8)
                .await
                .expect("converge shared RabbitMQ for two projects");
        assert!(
            first.logical_resource_drifts().is_empty(),
            "initial RabbitMQ convergence reported drift: {:?}",
            first.logical_resource_drifts()
        );
        assert_eq!(first.logical_resources().len(), 2);
        let container = owned_shared_container(&engine, &installation_id).await;

        let bill_output = publish_and_consume(
            &engine,
            &container,
            bill.definition().vhost(),
            bill.credential().username(),
            bill.credential().secret(),
            "bill-value",
        )
        .await
        .expect("publish and consume in bill RabbitMQ vhost");
        assert!(String::from_utf8_lossy(&bill_output).contains("bill-value"));
        let shop_output = publish_and_consume(
            &engine,
            &container,
            shop.definition().vhost(),
            shop.credential().username(),
            shop.credential().secret(),
            "shop-value",
        )
        .await
        .expect("publish and consume in shop RabbitMQ vhost");
        assert!(String::from_utf8_lossy(&shop_output).contains("shop-value"));
        assert!(matches!(
            rabbitmqadmin(
                &engine,
                &container,
                bill.definition().vhost(),
                shop.credential().username(),
                shop.credential().secret(),
                &["list", "queues"],
            )
            .await,
            Err(EngineError::ContainerExit { .. })
        ));

        let second =
            reconcile_prepared_rabbitmq_instance(&mut engine, prepared, &installation_id, 8)
                .await
                .expect("reconcile unchanged shared RabbitMQ instance");
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
                    .expect("persistent RabbitMQ volume")
                    .name()
                    .to_owned()],
            },
        )
        .await
        .expect("delete shared RabbitMQ acceptance resources");
    });

    drop(store);
    std::fs::remove_dir_all(&state_directory).expect("remove RabbitMQ acceptance state");
    println!("shared RabbitMQ isolation acceptance passed for {installation_id}");
}

async fn owned_shared_container(
    engine: &BollardEngineAdapter,
    installation_id: &str,
) -> OwnedContainer {
    engine
        .discover_managed()
        .await
        .expect("discover shared RabbitMQ acceptance resources")
        .into_iter()
        .filter_map(|observed| reconstruct_owned_container(&observed, installation_id, 8).ok())
        .find(|owned| owned.metadata().kind() == ResourceKind::SharedService)
        .expect("discover one owned shared RabbitMQ container")
}

async fn publish_and_consume(
    engine: &BollardEngineAdapter,
    container: &OwnedContainer,
    vhost: &str,
    username: &str,
    password: &str,
    value: &str,
) -> Result<Vec<u8>, EngineError> {
    rabbitmqadmin(
        engine,
        container,
        vhost,
        username,
        password,
        &[
            "declare",
            "queue",
            "--name",
            "stackctl_acceptance",
            "--durable",
            "true",
        ],
    )
    .await?;
    rabbitmqadmin(
        engine,
        container,
        vhost,
        username,
        password,
        &[
            "publish",
            "message",
            "--routing-key",
            "stackctl_acceptance",
            "--payload",
            value,
        ],
    )
    .await?;
    rabbitmqadmin(
        engine,
        container,
        vhost,
        username,
        password,
        &[
            "get",
            "messages",
            "--queue",
            "stackctl_acceptance",
            "--count",
            "1",
            "--ack-mode",
            "ack_requeue_false",
        ],
    )
    .await
}

async fn rabbitmqadmin(
    engine: &BollardEngineAdapter,
    container: &OwnedContainer,
    vhost: &str,
    username: &str,
    password: &str,
    arguments: &[&str],
) -> Result<Vec<u8>, EngineError> {
    let mut command = vec!["rabbitmqadmin".to_owned()];
    command.extend(arguments.iter().map(|argument| (*argument).to_owned()));
    let request = CommandRequest::new(
        command,
        BTreeMap::from([
            (
                "RABBITMQADMIN_TARGET_HOST".to_owned(),
                "127.0.0.1".to_owned(),
            ),
            ("RABBITMQADMIN_TARGET_PORT".to_owned(), "15672".to_owned()),
            ("RABBITMQADMIN_TARGET_VHOST".to_owned(), vhost.to_owned()),
            ("RABBITMQADMIN_USERNAME".to_owned(), username.to_owned()),
            ("RABBITMQADMIN_PASSWORD".to_owned(), password.to_owned()),
            (
                "RABBITMQADMIN_NON_INTERACTIVE_MODE".to_owned(),
                "true".to_owned(),
            ),
        ]),
        None,
    )?;
    let options = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "exercise RabbitMQ tenant isolation",
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
