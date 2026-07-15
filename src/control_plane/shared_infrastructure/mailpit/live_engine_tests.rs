use super::{
    MailpitPreparationOptions, prepare_mailpit_shared_instances,
    reconcile_prepared_mailpit_instance,
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
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAILPIT_IMAGE: &str = concat!(
    "axllent/mailpit@sha256:",
    "0059ef81e492a7192af3816281eed6859eb078bd7bdc58b76757c13e10e53a7d"
);

#[test]
#[ignore = "CI owns live shared Mailpit attribution acceptance"]
fn live_docker_engine_two_projects_share_one_mailpit_with_attributed_smtp() {
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
        architecture => panic!("unsupported Mailpit acceptance architecture '{architecture}'"),
    };
    let profile = CompatibilityProfile::from_options(CompatibilityFingerprintOptions {
        implementation: "mailpit".to_owned(),
        major_version: "1".to_owned(),
        image_digest: MAILPIT_IMAGE.to_owned(),
        extensions: Vec::new(),
        immutable_settings: BTreeMap::new(),
        persistence: PersistenceMode::Persistent,
        isolation: IsolationCapability::None,
        platform_architecture: Some(platform.to_owned()),
    })
    .expect("build Mailpit acceptance profile");
    let shared = plan_shared_instances(vec![
        SharedServiceRequest::new("bill", "mailpit", profile.clone()),
        SharedServiceRequest::new("shop", "mailpit", profile),
    ]);
    assert_eq!(shared.len(), 1, "compatible projects must share one plan");
    assert_eq!(shared[0].consumers().len(), 2);
    std::fs::create_dir(&state_directory).expect("create Mailpit acceptance state");
    let mut store = SqliteStateStore::open(&state_directory.join("state.sqlite3"))
        .expect("open Mailpit acceptance state store");
    let prepared = prepare_mailpit_shared_instances(
        &mut store,
        &shared,
        &SequentialCredentialEntropy::new(0x41),
        MailpitPreparationOptions {
            installation_id: &installation_id,
            network_name: &network_name,
            schema_version: 8,
            state_directory: &state_directory,
        },
    )
    .expect("prepare shared Mailpit acceptance resources");
    let replayed = prepare_mailpit_shared_instances(
        &mut store,
        &shared,
        &SequentialCredentialEntropy::new(0x81),
        MailpitPreparationOptions {
            installation_id: &installation_id,
            network_name: &network_name,
            schema_version: 8,
            state_directory: &state_directory,
        },
    )
    .expect("replay shared Mailpit acceptance preparation");
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
        .expect("prepared bill Mailpit resources");
    let shop = prepared
        .projects()
        .iter()
        .find(|project| project.environment().project_id() == "shop")
        .expect("prepared shop Mailpit resources");
    let network_metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.clone(),
        kind: ResourceKind::Network,
        project_id: None,
        compatibility_fingerprint: shared[0].fingerprint().as_str().to_owned(),
        schema_version: 8,
        desired_revision: shared[0].fingerprint().as_str().to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("build Mailpit acceptance network metadata");
    let network_request = NetworkCreateOptions::new(&network_name, network_metadata)
        .expect("build Mailpit acceptance network request");
    let image = ImmutableImageReference::new(MAILPIT_IMAGE)
        .expect("build immutable Mailpit acceptance image reference");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build Mailpit acceptance runtime");
    let mut engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    runtime.block_on(async {
        engine
            .ensure_image(&image)
            .await
            .expect("resolve immutable Mailpit acceptance image");
        engine
            .create_network(&network_request)
            .await
            .expect("create private Mailpit acceptance network");
        let first = reconcile_prepared_mailpit_instance(&mut engine, prepared, &installation_id, 8)
            .await
            .expect("converge shared Mailpit for two projects");
        assert!(first.logical_resource_drifts().is_empty());
        assert_eq!(first.logical_resources().len(), 2);
        let container = owned_shared_container(&engine, &installation_id).await;

        let bill_smtp = send_message(
            &engine,
            &container,
            bill.credential().username(),
            bill.credential().secret(),
            "bill acceptance",
        )
        .await
        .expect("send bill attributed SMTP message");
        assert!(bill_smtp.contains("235"), "SMTP auth failed: {bill_smtp}");
        assert!(
            bill_smtp.contains("queued"),
            "SMTP queue failed: {bill_smtp}"
        );
        let shop_smtp = send_message(
            &engine,
            &container,
            shop.credential().username(),
            shop.credential().secret(),
            "shop acceptance",
        )
        .await
        .expect("send shop attributed SMTP message");
        assert!(shop_smtp.contains("235"), "SMTP auth failed: {shop_smtp}");
        assert!(
            shop_smtp.contains("queued"),
            "SMTP queue failed: {shop_smtp}"
        );
        let rejected = send_message(
            &engine,
            &container,
            bill.credential().username(),
            shop.credential().secret(),
            "rejected acceptance",
        )
        .await
        .expect("receive SMTP authentication rejection");
        assert!(
            rejected.contains("535"),
            "wrong credential was accepted: {rejected}"
        );

        let messages = mailpit_messages(&engine, &container)
            .await
            .expect("read Mailpit acceptance messages");
        assert!(messages.contains("bill acceptance"));
        assert!(messages.contains("shop acceptance"));
        assert!(messages.contains(bill.credential().username()));
        assert!(messages.contains(shop.credential().username()));
        assert!(!messages.contains("rejected acceptance"));

        let second =
            reconcile_prepared_mailpit_instance(&mut engine, prepared, &installation_id, 8)
                .await
                .expect("reconcile unchanged shared Mailpit instance");
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
                    .expect("persistent Mailpit volume")
                    .name()
                    .to_owned()],
            },
        )
        .await
        .expect("delete shared Mailpit acceptance resources");
    });

    drop(store);
    std::fs::remove_dir_all(&state_directory).expect("remove Mailpit acceptance state");
    println!("shared Mailpit attribution acceptance passed for {installation_id}");
}

async fn owned_shared_container(
    engine: &BollardEngineAdapter,
    installation_id: &str,
) -> OwnedContainer {
    engine
        .discover_managed()
        .await
        .expect("discover shared Mailpit acceptance resources")
        .into_iter()
        .filter_map(|observed| reconstruct_owned_container(&observed, installation_id, 8).ok())
        .find(|owned| owned.metadata().kind() == ResourceKind::SharedService)
        .expect("discover one owned shared Mailpit container")
}

async fn send_message(
    engine: &BollardEngineAdapter,
    container: &OwnedContainer,
    username: &str,
    password: &str,
    subject: &str,
) -> Result<String, EngineError> {
    let authentication = STANDARD.encode(format!("\0{username}\0{password}"));
    let conversation = format!(
        "EHLO stackctl\r\n\
         AUTH PLAIN {authentication}\r\n\
         MAIL FROM:<sender@stackctl.localhost>\r\n\
         RCPT TO:<recipient@stackctl.localhost>\r\n\
         DATA\r\n\
         Subject: {subject}\r\n\
         \r\n\
         {subject}\r\n\
         .\r\n\
         QUIT\r\n"
    );
    let request = CommandRequest::new(
        vec![
            "nc".to_owned(),
            "-w".to_owned(),
            "5".to_owned(),
            "127.0.0.1".to_owned(),
            "1025".to_owned(),
        ],
        BTreeMap::new(),
        None,
    )?;
    let options = AttachedCommandOptions::new(
        request,
        conversation.into_bytes(),
        "exercise Mailpit SMTP attribution",
        Duration::from_secs(10),
    )?;
    let output = run_attached_command_capture(engine, container, &options).await?;

    String::from_utf8(output).map_err(|error| EngineError::Backend {
        detail: format!("Mailpit SMTP response is not UTF-8: {error}"),
    })
}

async fn mailpit_messages(
    engine: &BollardEngineAdapter,
    container: &OwnedContainer,
) -> Result<String, EngineError> {
    let request = CommandRequest::new(
        vec![
            "wget".to_owned(),
            "-qO-".to_owned(),
            "http://127.0.0.1:8025/api/v1/messages".to_owned(),
        ],
        BTreeMap::new(),
        None,
    )?;
    let options = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "read Mailpit attributed messages",
        Duration::from_secs(10),
    )?;
    let output = run_attached_command_capture(engine, container, &options).await?;

    String::from_utf8(output).map_err(|error| EngineError::Backend {
        detail: format!("Mailpit message response is not UTF-8: {error}"),
    })
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
