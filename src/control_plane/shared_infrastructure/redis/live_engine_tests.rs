use super::{
    RedisFlavor, RedisPreparationOptions, prepare_redis_shared_instances,
    reconcile_prepared_redis_instance,
};
use crate::control_plane::application::ControlPlane;
use crate::control_plane::daemon::{ProjectDiscoveryOptions, reconcile_watched_roots};
use crate::control_plane::engine::{
    AttachedCommandOptions, BollardEngineAdapter, CommandRequest, ContainerDiscovery,
    ImageResolver, ImmutableImageReference, InstallationResourceDeletionOptions,
    ManagedResourceMetadata, ManagedResourceMetadataOptions, NetworkCreateOptions, NetworkManager,
    OwnedContainer, ResourceKind, RetentionClass, delete_owned_installation_resources,
    reconstruct_owned_container, run_attached_command_output,
};
use crate::control_plane::shared_infrastructure::{
    CredentialEntropy, CredentialGenerationError, OrphanedSharedAccessOptions,
    resolve_execution_shared_instances, revoke_orphaned_shared_access_from_observed,
};
use crate::control_plane::state::{
    CredentialLifecycle, ResourceLifecycle, SqliteStateStore, StateStore,
};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const REDIS_IMAGE: &str = concat!(
    "redis@sha256:",
    "9d317178eceac8454a2284a9e6df2466b93c745529947f0cd42a0fa9609d7005"
);
const VALKEY_IMAGE: &str = concat!(
    "valkey/valkey@sha256:",
    "3e31dd49b6b742e614975e8ab7b1b19809d00ecac7657c6b34bff23582a433cd"
);

#[test]
#[ignore = "CI owns live shared Redis isolation acceptance"]
fn live_docker_engine_two_projects_share_one_redis_with_isolated_prefixes() {
    run_live_engine_shared_key_value_isolation(LiveEngineKeyValueOptions {
        flavor: RedisFlavor::Redis,
        display_name: "Redis",
        major_version: "8",
        image: REDIS_IMAGE,
    });
}

#[test]
#[ignore = "CI owns live shared Valkey isolation acceptance"]
fn live_docker_engine_two_projects_share_one_valkey_with_isolated_prefixes() {
    run_live_engine_shared_key_value_isolation(LiveEngineKeyValueOptions {
        flavor: RedisFlavor::Valkey,
        display_name: "Valkey",
        major_version: "8",
        image: VALKEY_IMAGE,
    });
}

fn run_live_engine_shared_key_value_isolation(options: LiveEngineKeyValueOptions<'_>) {
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
    let projects_root = state_directory.join("projects");
    let bill_directory = projects_root.join("bill");
    let shop_directory = projects_root.join("shop");
    let database_path = state_directory.join("state.sqlite3");
    let platform = match std::env::consts::ARCH {
        "aarch64" => "linux/arm64",
        "x86_64" => "linux/amd64",
        architecture => panic!(
            "unsupported {} acceptance architecture '{architecture}'",
            options.display_name
        ),
    };
    std::fs::create_dir_all(&bill_directory).expect("create bill Redis project");
    std::fs::create_dir_all(&shop_directory).expect("create shop Redis project");
    write_configuration(&bill_directory, "bill", options);
    write_configuration(&shop_directory, "shop", options);
    let mut store =
        SqliteStateStore::open(&database_path).expect("open Redis acceptance state store");
    store
        .replace_watched_roots(std::slice::from_ref(&projects_root))
        .expect("persist Redis acceptance watched root");
    drop(store);
    let mut control_plane = ControlPlane::new(
        SqliteStateStore::open(&database_path).expect("reopen Redis discovery state"),
    );
    let discovered = reconcile_watched_roots(
        &mut control_plane,
        ProjectDiscoveryOptions::bounded_defaults(),
        10_000,
    )
    .expect("discover Redis acceptance projects");
    let execution = crate::control_plane::resolve_execution_plan(
        discovered.registry().expect("complete Redis registry"),
    )
    .expect("resolve Redis acceptance execution");
    let shared = resolve_execution_shared_instances(&execution, platform)
        .expect("resolve Redis acceptance shared demand");
    drop(control_plane);
    assert_eq!(shared.len(), 1, "compatible projects must share one plan");
    assert_eq!(shared[0].consumers().len(), 2);
    let mut store = SqliteStateStore::open(&database_path).expect("reopen Redis preparation state");
    let prepared = prepare_redis_shared_instances(
        &mut store,
        &shared,
        &SequentialCredentialEntropy::new(0x11),
        RedisPreparationOptions {
            installation_id: &installation_id,
            network_name: &network_name,
            schema_version: 8,
            state_directory: &state_directory,
        },
    )
    .expect("prepare shared Redis acceptance resources");
    let replayed = prepare_redis_shared_instances(
        &mut store,
        &shared,
        &SequentialCredentialEntropy::new(0x51),
        RedisPreparationOptions {
            installation_id: &installation_id,
            network_name: &network_name,
            schema_version: 8,
            state_directory: &state_directory,
        },
    )
    .expect("replay shared Redis acceptance preparation");
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
    let instance = prepared.instance();
    let bill = prepared
        .projects()
        .iter()
        .find(|project| project.environment().project_id() == "bill")
        .expect("prepared bill Redis resources");
    let shop = prepared
        .projects()
        .iter()
        .find(|project| project.environment().project_id() == "shop")
        .expect("prepared shop Redis resources");
    let network_metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.clone(),
        kind: ResourceKind::Network,
        project_id: None,
        compatibility_fingerprint: shared[0].fingerprint().as_str().to_owned(),
        schema_version: 8,
        desired_revision: shared[0].fingerprint().as_str().to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("build Redis acceptance network metadata");
    let network_request = NetworkCreateOptions::new(&network_name, network_metadata)
        .expect("build Redis acceptance network request");
    let image = ImmutableImageReference::new(options.image)
        .expect("build immutable Redis-family acceptance image reference");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build Redis acceptance runtime");
    let mut engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    runtime.block_on(async {
        engine
            .ensure_image(&image)
            .await
            .expect("resolve immutable Redis acceptance image");
        engine
            .create_network(&network_request)
            .await
            .expect("create private Redis acceptance network");
        let first = reconcile_prepared_redis_instance(&mut engine, prepared, &installation_id, 8)
            .await
            .expect("converge shared Redis for two projects");
        let mut publication_control_plane = ControlPlane::new(
            SqliteStateStore::open(&database_path).expect("open Redis publication state"),
        );
        publication_control_plane
            .record_resources(first.physical_resources(), 11_000)
            .expect("publish Redis physical resources");
        for project in [bill, shop] {
            let logical = first
                .logical_resources()
                .iter()
                .filter(|logical| logical.project_id() == project.environment().project_id())
                .cloned()
                .collect::<Vec<_>>();
            publication_control_plane
                .reconcile_logical_environment(&logical, project.environment(), 11_000)
                .expect("publish Redis logical resources and environment");
        }
        drop(publication_control_plane);
        let container = owned_shared_container(&engine, &installation_id).await;

        let bill_key = format!("{}owned", bill.acl().prefix());
        let shop_key = format!("{}owned", shop.acl().prefix());
        assert_eq!(
            key_value_command(
                &engine,
                &container,
                options.flavor,
                bill.credential().username(),
                bill.credential().secret(),
                &["SET", &bill_key, "bill-value"],
            )
            .await,
            "OK\n"
        );
        assert_eq!(
            key_value_command(
                &engine,
                &container,
                options.flavor,
                shop.credential().username(),
                shop.credential().secret(),
                &["SET", &shop_key, "shop-value"],
            )
            .await,
            "OK\n"
        );
        let denied = key_value_command(
            &engine,
            &container,
            options.flavor,
            shop.credential().username(),
            shop.credential().secret(),
            &["GET", &bill_key],
        )
        .await;
        assert!(
            denied.contains("NOPERM"),
            "cross-project key access unexpectedly returned: {denied:?}"
        );

        let second = reconcile_prepared_redis_instance(&mut engine, prepared, &installation_id, 8)
            .await
            .expect("reconcile unchanged shared Redis instance");
        assert!(second.logical_resource_drifts().is_empty());
        let replayed_container = owned_shared_container(&engine, &installation_id).await;
        assert_eq!(replayed_container.id(), container.id());

        std::fs::remove_file(bill_directory.join(".stackctl.yaml"))
            .expect("remove bill Redis configuration");
        let mut removal_control_plane = ControlPlane::new(
            SqliteStateStore::open(&database_path).expect("open Redis removal state"),
        );
        let removed = reconcile_watched_roots(
            &mut removal_control_plane,
            ProjectDiscoveryOptions::bounded_defaults(),
            12_345,
        )
        .expect("reconcile removed bill Redis configuration");
        assert!(removed.was_applied());
        drop(removal_control_plane);
        let lifecycle_store =
            SqliteStateStore::open(&database_path).expect("inspect Redis orphan state");
        let bill_logical = lifecycle_store
            .logical_resources()
            .expect("load orphaned Redis resources")
            .into_iter()
            .find(|logical| logical.project_id() == "bill")
            .expect("find bill logical Redis resource");
        assert_eq!(bill_logical.lifecycle(), ResourceLifecycle::Orphaned);
        assert_eq!(bill_logical.orphaned_at_unix_seconds(), Some(12_345));
        let bill_disabled = lifecycle_store
            .credentials()
            .expect("load disabled Redis credential")
            .into_iter()
            .find(|credential| credential.project_id() == Some("bill"))
            .expect("find disabled bill Redis credential");
        assert_eq!(bill_disabled.lifecycle(), CredentialLifecycle::Disabled);
        assert_eq!(bill_disabled.secret(), bill.credential().secret());
        let credentials = [bill_disabled, instance.bootstrap_credential().clone()];
        let observed = engine
            .discover_managed()
            .await
            .expect("discover Redis lifecycle acceptance resources");
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
        .expect("revoke removed bill Redis access");
        assert_eq!(revoked, 1);

        let denied_after_removal = key_value_command(
            &engine,
            &container,
            options.flavor,
            bill.credential().username(),
            bill.credential().secret(),
            &["GET", &bill_key],
        )
        .await;
        assert!(
            denied_after_removal.contains("AUTH failed")
                || denied_after_removal.contains("WRONGPASS")
                || denied_after_removal.contains("NOAUTH"),
            "removed project credential remained usable: {denied_after_removal:?}"
        );
        assert_eq!(
            key_value_command(
                &engine,
                &container,
                options.flavor,
                shop.credential().username(),
                shop.credential().secret(),
                &["GET", &shop_key],
            )
            .await,
            "shop-value\n"
        );
        assert_eq!(
            key_value_command(
                &engine,
                &container,
                options.flavor,
                instance.bootstrap_credential().username(),
                instance.bootstrap_credential().secret(),
                &["GET", &bill_key],
            )
            .await,
            "bill-value\n",
            "credential revocation must retain the removed project's data"
        );

        write_configuration(&bill_directory, "bill", options);
        let bill_directory = bill_directory
            .canonicalize()
            .expect("canonical restored bill project");
        let mut restoration_control_plane = ControlPlane::new(
            SqliteStateStore::open(&database_path).expect("open Redis restoration state"),
        );
        let restored_registry = reconcile_watched_roots(
            &mut restoration_control_plane,
            ProjectDiscoveryOptions::bounded_defaults(),
            15_000,
        )
        .expect("rediscover bill Redis configuration");
        assert!(restored_registry.was_applied());
        assert_eq!(
            restoration_control_plane
                .adopt_project(&bill_directory)
                .expect("adopt restored bill Redis project"),
            "bill"
        );
        drop(restoration_control_plane);
        let restored_prepared = prepare_redis_shared_instances(
            &mut store,
            &shared,
            &SequentialCredentialEntropy::new(0x71),
            RedisPreparationOptions {
                installation_id: &installation_id,
                network_name: &network_name,
                schema_version: 8,
                state_directory: &state_directory,
            },
        )
        .expect("prepare adopted Redis resources");
        let restored_bill = restored_prepared[0]
            .projects()
            .iter()
            .find(|project| project.environment().project_id() == "bill")
            .expect("restored bill Redis resources");
        assert_eq!(restored_bill.credential(), bill.credential());
        let restored = reconcile_prepared_redis_instance(
            &mut engine,
            &restored_prepared[0],
            &installation_id,
            8,
        )
        .await
        .expect("restore bill Redis access from the adopted configuration");
        assert!(restored.logical_resource_drifts().is_empty());
        assert_eq!(
            key_value_command(
                &engine,
                &container,
                options.flavor,
                restored_bill.credential().username(),
                restored_bill.credential().secret(),
                &["GET", &bill_key],
            )
            .await,
            "bill-value\n"
        );
        let restored_container = owned_shared_container(&engine, &installation_id).await;
        assert_eq!(restored_container.id(), container.id());

        delete_owned_installation_resources(
            &mut engine,
            InstallationResourceDeletionOptions {
                installation_id: &installation_id,
                schema_version: 8,
                authorized_persistent_volumes: &[instance
                    .volume()
                    .expect("persistent Redis volume")
                    .name()
                    .to_owned()],
            },
        )
        .await
        .expect("delete shared Redis acceptance resources");
    });

    drop(store);
    std::fs::remove_dir_all(&state_directory).expect("remove Redis acceptance state");
    println!(
        "shared {} isolation acceptance passed for installation {installation_id}",
        options.display_name
    );
}

async fn owned_shared_container(
    engine: &BollardEngineAdapter,
    installation_id: &str,
) -> OwnedContainer {
    let owned = engine
        .discover_managed()
        .await
        .expect("discover shared Redis acceptance resources")
        .into_iter()
        .filter_map(|observed| reconstruct_owned_container(&observed, installation_id, 8).ok())
        .filter(|owned| owned.metadata().kind() == ResourceKind::SharedService)
        .collect::<Vec<_>>();
    assert_eq!(owned.len(), 1);

    owned
        .into_iter()
        .next()
        .expect("one shared Redis container")
}

fn write_configuration(
    directory: &std::path::Path,
    project: &str,
    options: LiveEngineKeyValueOptions<'_>,
) {
    std::fs::write(
        directory.join(".stackctl.yaml"),
        format!(
            concat!(
                "schema_version: 8\nproject: {project}\nservices:\n",
                "  cache:\n    preset: {preset}\n    version: '{version}'\n",
                "    image: {image}\n"
            ),
            project = project,
            preset = options.flavor.implementation(),
            version = options.major_version,
            image = options.image,
        ),
    )
    .expect("write Redis acceptance configuration");
}

#[derive(Clone, Copy)]
struct LiveEngineKeyValueOptions<'a> {
    flavor: RedisFlavor,
    display_name: &'a str,
    major_version: &'a str,
    image: &'a str,
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

async fn key_value_command(
    engine: &BollardEngineAdapter,
    container: &OwnedContainer,
    flavor: RedisFlavor,
    username: &str,
    password: &str,
    arguments: &[&str],
) -> String {
    let mut command = vec![
        flavor.client_executable().to_owned(),
        "--user".to_owned(),
        username.to_owned(),
        "--raw".to_owned(),
    ];
    command.extend(arguments.iter().map(ToString::to_string));
    let request = CommandRequest::new(
        command,
        BTreeMap::from([(
            flavor.client_auth_environment_key().to_owned(),
            password.to_owned(),
        )]),
        None,
    )
    .expect("build Redis tenant command");
    let options = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "exercise Redis tenant isolation",
        Duration::from_secs(10),
    )
    .expect("build bounded Redis tenant command");
    let output = run_attached_command_output(engine, container, &options)
        .await
        .expect("execute Redis tenant command");
    let mut bytes = output.stdout().to_vec();
    bytes.extend_from_slice(output.stderr());

    String::from_utf8(bytes).expect("Redis tenant output must be UTF-8")
}
