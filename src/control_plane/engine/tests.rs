use super::{
    AttachedCommandOptions, BindMount, BollardEngineAdapter, CommandExecutor, CommandRequest,
    ContainerCompletion, ContainerCreateOptions, ContainerDiscovery, ContainerEvent,
    ContainerEventAction, ContainerEventCursor, ContainerEventSource, ContainerEventStream,
    ContainerHealth, ContainerHealthCheck, ContainerId, ContainerLifecycle, ContainerLogOptions,
    ContainerLogStream, ContainerLogTail, ContainerNetworkIsolation, ContainerResourceMetrics,
    ContainerState, EngineError, EngineFuture, GatewayContainerRequestOptions, HealthObserver,
    ImageBuildRequest, ImageBuilder, ImageDiscovery, ImageId, ImageManager, ImageReferenceResolver,
    ImageResolver, ImmutableImageReference, InstallationResourceDeletionOptions, LogChunk,
    LogSource, ManagedResourceMetadata, ManagedResourceMetadataOptions, NetworkCreateOptions,
    NetworkDiscovery, NetworkId, NetworkManager, ObservedContainer, ObservedImage, ObservedNetwork,
    ObservedResourceOwnership, ObservedVolume, OwnedContainer, OwnedImage, OwnedNetwork,
    OwnedVolume, PublishedPortBinding, PublishedPortDiscovery, ReconciliationEngine,
    RegistryImageReference, ResourceKind, ResourceMetrics, RetentionClass, VolumeCreateOptions,
    VolumeDiscovery, VolumeManager, VolumeMount, classify_observed_resource,
    delete_owned_installation_resources, gateway_container_request, reconstruct_owned_container,
    reconstruct_owned_image, reconstruct_owned_network, reconstruct_owned_volume,
    run_attached_command_capture, validate_container_completion,
};
use bollard::ClientVersion;
use bollard::container::LogOutput;
use bollard::errors::Error as BollardError;
use bollard::models::{
    ContainerCpuStats, ContainerCpuUsage, ContainerMemoryStats, ContainerNetworkStats,
    ContainerPidsStats, ContainerState as EngineContainerState, ContainerStatsResponse,
    ContainerSummary, EventActor, EventMessage, EventMessageTypeEnum, Health, HealthStatusEnum,
    ImageSummary, MountPoint, Network, PortSummary, PortSummaryTypeEnum, Volume,
};
use futures_util::StreamExt;
use std::collections::BTreeMap;
use std::future::pending;
use std::io::Read;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::bollard_engine_adapter::{
    build_image_options, canonical_command_container_id, command_create_request, container_event,
    container_health, container_resource_metrics, container_wait_error, create_request,
    exact_volume_mount_target, image_pull_request, log_chunk, log_request,
    managed_container_events_request, managed_container_list_request, managed_image_list_request,
    managed_network_list_request, managed_volume_list_request, network_create_request,
    observed_container, observed_image, observed_network, observed_volume, published_port_bindings,
    published_port_list_request, validate_engine_api_version, validate_volume_archive_identity,
    verify_owned_container_labels, verify_owned_network_labels, verify_owned_volume_labels,
    volume_archive_subpath_target, volume_archive_subpath_upload_target,
    volume_archive_upload_target, volume_create_request,
};
use super::bounded_engine_operation::bounded_engine_operation;

#[test]
#[ignore = "CI owns live Docker Engine API acceptance"]
fn live_docker_engine_adapter_negotiates_and_reads_owned_inventory() {
    let socket = std::env::var_os("STACKCTL_ENGINE_SOCKET")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/var/run/docker.sock"));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build Engine acceptance runtime");
    let engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    runtime.block_on(async {
        engine
            .discover_managed()
            .await
            .expect("read managed container inventory");
        engine
            .discover_managed_networks()
            .await
            .expect("read managed network inventory");
        engine
            .discover_managed_images()
            .await
            .expect("read managed image inventory");
        engine
            .discover_managed_volumes()
            .await
            .expect("read managed volume inventory");
        engine
            .discover_published_tcp_ports()
            .await
            .expect("read published port inventory");
    });
}

#[test]
#[ignore = "CI owns live Docker Engine persistent-data acceptance"]
fn live_docker_engine_persistent_volume_deletion_requires_exact_authorization() {
    let socket = std::env::var_os("STACKCTL_ENGINE_SOCKET")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/var/run/docker.sock"));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must follow the Unix epoch")
        .as_nanos();
    let installation_id = format!("ci-{}-{nonce}", std::process::id());
    let volume_name = format!("stackctl-{installation_id}-project-data");
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.clone(),
        kind: ResourceKind::Volume,
        project_id: Some("ci-project".to_owned()),
        compatibility_fingerprint: format!("sha256:{}", "a".repeat(64)),
        schema_version: 8,
        desired_revision: format!("sha256:{}", "b".repeat(64)),
        retention: RetentionClass::Persistent,
    })
    .and_then(|metadata| metadata.with_resource_id("data"))
    .expect("build persistent CI volume metadata");
    let request = VolumeCreateOptions::new(&volume_name, metadata)
        .expect("build persistent CI volume request");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build Engine persistent-data acceptance runtime");
    let mut engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    runtime.block_on(async {
        engine
            .create_volume(&request)
            .await
            .expect("create owned persistent CI volume");

        let error = delete_owned_installation_resources(
            &mut engine,
            InstallationResourceDeletionOptions {
                installation_id: &installation_id,
                schema_version: 8,
                authorized_persistent_volumes: &[],
            },
        )
        .await
        .expect_err("persistent project volume must require exact authorization");
        assert!(
            error
                .to_string()
                .contains("without exact recovery authorization")
        );
        assert!(
            engine
                .discover_managed_volumes()
                .await
                .expect("inspect retained CI volume")
                .iter()
                .any(|volume| volume.name() == volume_name)
        );

        delete_owned_installation_resources(
            &mut engine,
            InstallationResourceDeletionOptions {
                installation_id: &installation_id,
                schema_version: 8,
                authorized_persistent_volumes: std::slice::from_ref(&volume_name),
            },
        )
        .await
        .expect("delete exactly authorized persistent CI volume");
        assert!(
            engine
                .discover_managed_volumes()
                .await
                .expect("verify persistent CI volume deletion")
                .iter()
                .all(|volume| volume.name() != volume_name)
        );
    });

    println!(
        "persistent-volume authorization acceptance passed for installation {installation_id}"
    );
}

#[test]
#[ignore = "CI owns live Docker Engine workload-isolation acceptance"]
fn live_docker_engine_project_application_uses_private_network_without_host_ports() {
    const BUSYBOX_IMAGE: &str = concat!(
        "busybox@sha256:",
        "9532d8c39891ca2ecde4d30d7710e01fb739c87a8b9299685c63704296b16028"
    );

    let socket = std::env::var_os("STACKCTL_ENGINE_SOCKET")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/var/run/docker.sock"));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must follow the Unix epoch")
        .as_nanos();
    let installation_id = format!("ci-{}-{nonce}", std::process::id());
    let network_name = format!("stackctl-{installation_id}");
    let container_name = format!("stackctl-{installation_id}-app");
    let fingerprint = format!("sha256:{}", "c".repeat(64));
    let revision = format!("sha256:{}", "d".repeat(64));
    let network_metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.clone(),
        kind: ResourceKind::Network,
        project_id: None,
        compatibility_fingerprint: fingerprint.clone(),
        schema_version: 8,
        desired_revision: revision.clone(),
        retention: RetentionClass::Persistent,
    })
    .expect("build CI network metadata");
    let application_metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.clone(),
        kind: ResourceKind::ProjectApplication,
        project_id: Some("ci-project".to_owned()),
        compatibility_fingerprint: fingerprint,
        schema_version: 8,
        desired_revision: revision,
        retention: RetentionClass::Disposable,
    })
    .and_then(|metadata| metadata.with_resource_id("app"))
    .expect("build CI application metadata");
    let network_request = NetworkCreateOptions::new(&network_name, network_metadata)
        .expect("build CI network request");
    let container_request =
        ContainerCreateOptions::new(&container_name, BUSYBOX_IMAGE, application_metadata)
            .and_then(|request| request.with_network(&network_name))
            .and_then(|request| {
                request.with_command(vec![
                    "sh".to_owned(),
                    "-c".to_owned(),
                    "while :; do sleep 30; done".to_owned(),
                ])
            })
            .expect("build private CI application request");
    let image = ImmutableImageReference::new(BUSYBOX_IMAGE).expect("build immutable CI image");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build Engine workload-isolation acceptance runtime");
    let mut engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    runtime.block_on(async {
        engine
            .ensure_image(&image)
            .await
            .expect("resolve immutable CI application image");
        let network = engine
            .create_network(&network_request)
            .await
            .expect("create owned private CI network");
        let container = engine
            .create(&container_request)
            .await
            .expect("create owned private CI application");
        engine
            .start(&container)
            .await
            .expect("start private CI application");

        assert_eq!(
            engine
                .inspect(&container)
                .await
                .expect("inspect private CI application"),
            ContainerState::Running
        );
        let observed = engine
            .discover_managed()
            .await
            .expect("discover private CI application");
        let owned = observed
            .iter()
            .find(|observed| observed.id() == container.id())
            .and_then(|observed| reconstruct_owned_container(observed, &installation_id, 8).ok())
            .expect("reconstruct exact CI application ownership");
        assert_eq!(owned.metadata().kind(), ResourceKind::ProjectApplication);
        assert_eq!(owned.metadata().project_id(), Some("ci-project"));
        assert_eq!(owned.metadata().resource_id(), Some("app"));
        assert!(
            engine
                .discover_published_tcp_ports()
                .await
                .expect("inspect CI application host ports")
                .iter()
                .all(|binding| binding.container_id() != container.id())
        );
        let observed_networks = engine
            .discover_managed_networks()
            .await
            .expect("discover private CI network");
        let owned_network = observed_networks
            .iter()
            .find(|observed| observed.id() == network.id())
            .and_then(|observed| reconstruct_owned_network(observed, &installation_id, 8).ok())
            .expect("reconstruct exact CI network ownership");
        assert_eq!(owned_network.metadata().kind(), ResourceKind::Network);
        assert_eq!(owned_network.metadata().project_id(), None);

        delete_owned_installation_resources(
            &mut engine,
            InstallationResourceDeletionOptions {
                installation_id: &installation_id,
                schema_version: 8,
                authorized_persistent_volumes: &[],
            },
        )
        .await
        .expect("delete disposable CI application and private network");
        assert_eq!(
            engine
                .inspect(&container)
                .await
                .expect("verify CI application deletion"),
            ContainerState::Missing
        );
    });

    println!("private workload acceptance passed for installation {installation_id}");
}

#[test]
#[ignore = "CI owns live Docker Engine cross-project network isolation acceptance"]
fn live_docker_engine_projects_are_network_isolated_behind_one_gateway() {
    const BUSYBOX_IMAGE: &str = concat!(
        "busybox@sha256:",
        "9532d8c39891ca2ecde4d30d7710e01fb739c87a8b9299685c63704296b16028"
    );

    let socket = std::env::var_os("STACKCTL_ENGINE_SOCKET")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/var/run/docker.sock"));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must follow the Unix epoch")
        .as_nanos();
    let installation_id = format!("isolation-{}-{nonce}", std::process::id());
    let global_name = format!("stackctl-{installation_id}");
    let bill_network_name = format!("{global_name}-bill");
    let ship_network_name = format!("{global_name}-ship");
    let bill_name = format!("{installation_id}-bill-app");
    let ship_name = format!("{installation_id}-ship-app");
    let shared_name = format!("{installation_id}-shared");
    let gateway_name = format!("{installation_id}-gateway");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build cross-project isolation acceptance runtime");
    let mut engine = runtime
        .block_on(BollardEngineAdapter::connect_unix(&socket))
        .expect("negotiate the selected Docker Engine API");

    let acceptance = runtime.block_on(async {
        engine
            .ensure_image(&ImmutableImageReference::new(BUSYBOX_IMAGE)?)
            .await?;
        let _global = engine
            .create_network(&acceptance_network_request(
                &installation_id,
                &global_name,
                None,
            )?)
            .await?;
        let bill_network = engine
            .create_network(&acceptance_network_request(
                &installation_id,
                &bill_network_name,
                Some("bill"),
            )?)
            .await?;
        let ship_network = engine
            .create_network(&acceptance_network_request(
                &installation_id,
                &ship_network_name,
                Some("ship"),
            )?)
            .await?;
        let bill = engine
            .create(&acceptance_http_container(
                &installation_id,
                &bill_name,
                BUSYBOX_IMAGE,
                ResourceKind::ProjectApplication,
                Some("bill"),
                &bill_network_name,
                "bill",
            )?)
            .await?;
        let ship = engine
            .create(&acceptance_http_container(
                &installation_id,
                &ship_name,
                BUSYBOX_IMAGE,
                ResourceKind::ProjectApplication,
                Some("ship"),
                &ship_network_name,
                "ship",
            )?)
            .await?;
        let shared = engine
            .create(&acceptance_http_container(
                &installation_id,
                &shared_name,
                BUSYBOX_IMAGE,
                ResourceKind::SharedService,
                None,
                &global_name,
                "shared",
            )?)
            .await?;
        let gateway = engine
            .create(&acceptance_idle_container(
                &installation_id,
                &gateway_name,
                BUSYBOX_IMAGE,
                ResourceKind::Gateway,
                &global_name,
            )?)
            .await?;
        for container in [&bill, &ship, &shared, &gateway] {
            engine.start(container).await?;
        }
        engine
            .reconnect_container_network(&shared, &bill_network, &shared_name)
            .await?;
        engine
            .reconnect_container_network(&gateway, &bill_network, &gateway_name)
            .await?;
        engine
            .reconnect_container_network(&gateway, &ship_network, &gateway_name)
            .await?;
        tokio::time::sleep(Duration::from_millis(200)).await;

        let bill_self = acceptance_http_probe(&engine, &bill, &bill_name).await?;
        let bill_shared = acceptance_http_probe(&engine, &bill, &shared_name).await?;
        let bill_to_ship = acceptance_http_probe(&engine, &bill, &ship_name).await;
        let ship_self = acceptance_http_probe(&engine, &ship, &ship_name).await?;
        let ship_to_bill = acceptance_http_probe(&engine, &ship, &bill_name).await;
        let ship_to_shared = acceptance_http_probe(&engine, &ship, &shared_name).await;
        let gateway_to_bill = acceptance_http_probe(&engine, &gateway, &bill_name).await?;
        let gateway_to_ship = acceptance_http_probe(&engine, &gateway, &ship_name).await?;

        Ok::<_, EngineError>((
            bill_self,
            bill_shared,
            bill_to_ship,
            ship_self,
            ship_to_bill,
            ship_to_shared,
            gateway_to_bill,
            gateway_to_ship,
        ))
    });
    runtime
        .block_on(delete_owned_installation_resources(
            &mut engine,
            InstallationResourceDeletionOptions {
                installation_id: &installation_id,
                schema_version: 8,
                authorized_persistent_volumes: &[],
            },
        ))
        .expect("delete cross-project isolation acceptance resources");
    let (
        bill_self,
        bill_shared,
        bill_to_ship,
        ship_self,
        ship_to_bill,
        ship_to_shared,
        gateway_to_bill,
        gateway_to_ship,
    ) = acceptance.expect("execute cross-project isolation acceptance");

    assert_eq!(bill_self, b"bill\n");
    assert_eq!(bill_shared, b"shared\n");
    assert!(bill_to_ship.is_err(), "bill must not reach ship directly");
    assert_eq!(ship_self, b"ship\n");
    assert!(ship_to_bill.is_err(), "ship must not reach bill directly");
    assert!(
        ship_to_shared.is_err(),
        "ship must not reach bill's authorized shared service"
    );
    assert_eq!(gateway_to_bill, b"bill\n");
    assert_eq!(gateway_to_ship, b"ship\n");
}

fn acceptance_network_request(
    installation_id: &str,
    name: &str,
    project_id: Option<&str>,
) -> Result<NetworkCreateOptions, EngineError> {
    let metadata = acceptance_metadata(
        installation_id,
        ResourceKind::Network,
        project_id,
        "private",
    )?;

    NetworkCreateOptions::new(name, metadata)
}

fn acceptance_http_container(
    installation_id: &str,
    name: &str,
    image: &str,
    kind: ResourceKind,
    project_id: Option<&str>,
    network: &str,
    response: &str,
) -> Result<ContainerCreateOptions, EngineError> {
    ContainerCreateOptions::new(
        name,
        image,
        acceptance_metadata(installation_id, kind, project_id, "app")?,
    )?
    .with_network(network)?
    .with_command(vec![
        "sh".to_owned(),
        "-c".to_owned(),
        format!(
            "mkdir -p /www && printf '%s\\n' '{response}' > /www/index.html && exec httpd -f -p 8080 -h /www"
        ),
    ])
}

fn acceptance_idle_container(
    installation_id: &str,
    name: &str,
    image: &str,
    kind: ResourceKind,
    network: &str,
) -> Result<ContainerCreateOptions, EngineError> {
    ContainerCreateOptions::new(
        name,
        image,
        acceptance_metadata(installation_id, kind, None, "gateway")?,
    )?
    .with_network(network)?
    .with_command(vec![
        "sh".to_owned(),
        "-c".to_owned(),
        "exec sleep 300".to_owned(),
    ])
}

fn acceptance_metadata(
    installation_id: &str,
    kind: ResourceKind,
    project_id: Option<&str>,
    resource_id: &str,
) -> Result<ManagedResourceMetadata, EngineError> {
    ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: installation_id.to_owned(),
        kind,
        project_id: project_id.map(str::to_owned),
        compatibility_fingerprint: "isolation-v1".to_owned(),
        schema_version: 8,
        desired_revision: "isolation-v1".to_owned(),
        retention: RetentionClass::Disposable,
    })?
    .with_resource_id(resource_id)
}

async fn acceptance_http_probe(
    engine: &BollardEngineAdapter,
    container: &OwnedContainer,
    hostname: &str,
) -> Result<Vec<u8>, EngineError> {
    let command = AttachedCommandOptions::new(
        CommandRequest::new(
            vec![
                "wget".to_owned(),
                "-q".to_owned(),
                "-O".to_owned(),
                "-".to_owned(),
                "-T".to_owned(),
                "2".to_owned(),
                format!("http://{hostname}:8080/"),
            ],
            BTreeMap::new(),
            None,
        )?,
        Vec::new(),
        "probe isolated HTTP endpoint",
        Duration::from_secs(5),
    )?;

    run_attached_command_capture(engine, container, &command).await
}

#[test]
fn reconciliation_engine_reuses_pass_wide_resource_observations() {
    let containers = [ObservedContainer::new(
        ContainerId::new("shared-postgres"),
        BTreeMap::new(),
    )];
    let volumes = [ObservedVolume::new("shared-postgres-data", BTreeMap::new())];
    let networks = [ObservedNetwork::new(
        NetworkId::new("stackctl-network"),
        BTreeMap::new(),
    )];
    let engine = ReconciliationEngine::new(
        RecordingContainerBackend::default(),
        &containers,
        &volumes,
        &networks,
    );
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("build test runtime");

    let (observed_containers, observed_volumes, observed_networks) = runtime.block_on(async {
        (
            engine.discover_managed().await.expect("cached containers"),
            engine
                .discover_managed_volumes()
                .await
                .expect("cached volumes"),
            engine
                .discover_managed_networks()
                .await
                .expect("cached networks"),
        )
    });

    assert_eq!(observed_containers[0].id().as_str(), "shared-postgres");
    assert_eq!(observed_volumes[0].name(), "shared-postgres-data");
    assert_eq!(observed_networks[0].id().as_str(), "stackctl-network");
}

#[test]
fn managed_metadata_generates_complete_reserved_ownership_labels() {
    let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::SharedService,
        project_id: Some("bill".to_owned()),
        compatibility_fingerprint: "sha256:abc123".to_owned(),
        schema_version: 8,
        desired_revision: "sha256:def456".to_owned(),
        retention: RetentionClass::Persistent,
    })
    .and_then(|metadata| metadata.with_compatibility_profile("postgresql", "17"))
    .expect("valid managed metadata");

    assert_eq!(
        metadata.labels(),
        BTreeMap::from([
            (
                "dev.stackctl.compatibility.implementation".to_owned(),
                "postgresql".to_owned()
            ),
            (
                "dev.stackctl.compatibility.major-version".to_owned(),
                "17".to_owned()
            ),
            (
                "dev.stackctl.fingerprint".to_owned(),
                "sha256:abc123".to_owned()
            ),
            (
                "dev.stackctl.installation".to_owned(),
                "install-1".to_owned()
            ),
            ("dev.stackctl.kind".to_owned(), "shared_service".to_owned()),
            ("dev.stackctl.managed".to_owned(), "true".to_owned()),
            ("dev.stackctl.project".to_owned(), "bill".to_owned()),
            ("dev.stackctl.schema".to_owned(), "8".to_owned()),
            (
                "dev.stackctl.desired".to_owned(),
                "sha256:def456".to_owned()
            ),
            ("dev.stackctl.retention".to_owned(), "persistent".to_owned()),
        ])
    );
}

#[test]
fn observed_project_process_ownership_requires_a_stable_resource_identity() {
    let labels = project_metadata(ResourceKind::ProjectProcess).labels();

    let ownership = classify_observed_resource(&labels, "install-1", 8);

    assert_eq!(
        ownership,
        ObservedResourceOwnership::Malformed {
            detail: "managed resource label 'dev.stackctl.resource' is missing".to_owned(),
        }
    );
}

#[test]
fn container_lifecycle_is_an_object_safe_replaceable_strategy() {
    let metadata = project_metadata(ResourceKind::ProjectApplication);
    let options = ContainerCreateOptions::new(
        "stackctl-bill-app",
        concat!(
            "ghcr.io/stackctl/php@sha256:",
            "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
        ),
        metadata,
    )
    .expect("immutable container options");
    let mut backend = RecordingContainerBackend::default();

    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");
    let container = runtime
        .block_on(create_through_strategy(&mut backend, &options))
        .expect("create container");

    assert_eq!(container.id().as_str(), "container-1");
    assert_eq!(backend.created, vec![options]);
}

#[test]
fn container_completion_is_an_object_safe_bounded_strategy() {
    let container = OwnedContainer::new(
        ContainerId::new("container-1"),
        project_metadata(ResourceKind::ProjectApplication),
    );
    let backend = RecordingContainerCompletion;
    let strategy: &dyn ContainerCompletion = &backend;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    runtime
        .block_on(strategy.wait_for_success(&container, Duration::from_secs(30)))
        .expect("successful container completion");
}

#[test]
fn container_completion_rejects_nonzero_exit_status() {
    let error = validate_container_completion("provisioning-job-1", 23)
        .expect_err("nonzero completion status");

    assert_eq!(
        error,
        EngineError::ContainerExit {
            container_id: "provisioning-job-1".to_owned(),
            status_code: 23,
        }
    );
    assert_eq!(
        error.to_string(),
        "container 'provisioning-job-1' exited with status 23"
    );
}

#[test]
fn container_completion_accepts_zero_exit_status() {
    validate_container_completion("provisioning-job-1", 0).expect("zero completion status");
}

#[test]
fn docker_wait_exit_errors_preserve_the_container_status() {
    let error = container_wait_error(
        "provisioning-job-1",
        BollardError::DockerContainerWaitError {
            error: String::new(),
            code: 42,
        },
    );

    assert_eq!(
        error,
        EngineError::ContainerExit {
            container_id: "provisioning-job-1".to_owned(),
            status_code: 42,
        }
    );
}

#[test]
fn container_mutation_rejects_missing_or_changed_ownership_labels() {
    let container = OwnedContainer::new(
        ContainerId::new("container-1"),
        project_metadata(ResourceKind::ProjectApplication),
    );

    let error = verify_owned_container_labels(&container, &std::collections::HashMap::new())
        .expect_err("unlabelled container");

    assert_eq!(
        error.to_string(),
        "refusing to mutate container 'container-1' because its Engine ownership labels no longer match"
    );
}

#[test]
fn mutable_image_tags_are_rejected_before_an_engine_request() {
    let metadata = global_metadata(ResourceKind::Gateway);

    let error = ContainerCreateOptions::new("stackctl-gateway", "caddy:latest", metadata)
        .expect_err("mutable image reference");

    assert_eq!(
        error.to_string(),
        "managed image 'caddy:latest' must use an immutable sha256 digest"
    );
}

#[test]
fn derived_image_content_ids_are_valid_container_inputs() {
    let image_id = format!("sha256:{}", "a".repeat(64));

    let options = ContainerCreateOptions::new(
        "stackctl-bill-app",
        image_id.clone(),
        project_metadata(ResourceKind::ProjectApplication),
    )
    .expect("local immutable image ID");

    assert_eq!(options.image(), image_id);
}

#[test]
fn malformed_engine_image_ids_are_rejected_at_the_boundary() {
    let error = ImageId::new("sha256:image-config").expect_err("malformed Engine image ID");

    assert_eq!(
        error.to_string(),
        "Engine image ID 'sha256:image-config' must be a sha256 content identity"
    );
}

#[test]
fn image_resolution_accepts_only_immutable_digest_references() {
    let immutable = ImmutableImageReference::new(concat!(
        "ghcr.io/stackctl/php@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    ))
    .expect("immutable image");
    let error =
        ImmutableImageReference::new("ghcr.io/stackctl/php:8.4").expect_err("mutable image tag");
    let local_id = format!("sha256:{}", "a".repeat(64));
    let local_id_error =
        ImmutableImageReference::new(local_id.clone()).expect_err("local content ID pull");
    let unsafe_reference = ImmutableImageReference::new(format!(
        "php AS injected\nRUN exploit@sha256:{}",
        "a".repeat(64)
    ))
    .expect_err("Dockerfile control text");
    let mut resolver = RecordingImageResolver::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");
    let strategy: &mut dyn ImageResolver = &mut resolver;

    let image = runtime
        .block_on(strategy.ensure_image(&immutable))
        .expect("resolve image");

    assert_eq!(image.as_str(), format!("sha256:{}", "a".repeat(64)));
    assert_eq!(resolver.resolved, vec![immutable]);
    assert_eq!(
        error.to_string(),
        "managed image 'ghcr.io/stackctl/php:8.4' must use an immutable sha256 digest"
    );
    assert_eq!(
        local_id_error.to_string(),
        format!("managed image '{local_id}' must use an immutable sha256 digest")
    );
    assert!(
        unsafe_reference
            .to_string()
            .contains("must use an immutable sha256 digest")
    );
}

#[test]
fn registry_image_resolution_preserves_the_source_and_pins_its_repository() {
    let reference = RegistryImageReference::new("ghcr.io/stackctl/php:8.4")
        .expect("mutable registry reference");
    let localhost = RegistryImageReference::new("localhost:5000/team/php")
        .expect("registry reference without a tag");
    let mut resolver = RecordingImageReferenceResolver::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");
    let strategy: &mut dyn ImageReferenceResolver = &mut resolver;

    let resolved = runtime
        .block_on(strategy.resolve_image_reference(&reference))
        .expect("resolve manifest reference");

    assert_eq!(reference.as_str(), "ghcr.io/stackctl/php:8.4");
    assert_eq!(reference.repository(), "ghcr.io/stackctl/php");
    assert_eq!(localhost.repository(), "localhost:5000/team/php");
    assert_eq!(
        resolved.as_str(),
        concat!(
            "ghcr.io/stackctl/php@sha256:",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        )
    );
    assert_eq!(resolver.resolved, vec![reference]);
}

#[test]
fn mutable_registry_image_boundary_rejects_ambiguous_or_immutable_input() {
    for source in [
        "",
        " ghcr.io/stackctl/php:8.4",
        "ghcr.io/stackctl/php:",
        concat!(
            "ghcr.io/stackctl/php@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ),
    ] {
        let error = RegistryImageReference::new(source).expect_err("invalid mutable reference");

        assert!(error.to_string().contains("registry image reference"));
    }

    let reference = RegistryImageReference::new("ghcr.io/stackctl/php:8.4")
        .expect("mutable registry reference");
    let error = reference
        .with_digest("sha256:short")
        .expect_err("malformed manifest digest");

    assert!(error.to_string().contains("immutable sha256 digest"));
}

#[test]
fn image_pull_requests_preserve_the_complete_digest_reference() {
    let reference = ImmutableImageReference::new(concat!(
        "ghcr.io/stackctl/php@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    ))
    .expect("immutable image");

    let request = image_pull_request(&reference);

    assert_eq!(request.from_image.as_deref(), Some(reference.as_str()));
    assert_eq!(request.tag, None);
}

#[test]
fn managed_container_events_are_scoped_and_resume_without_replaying_the_cursor() {
    let processed = ContainerEvent::new(
        ContainerId::new("container-1"),
        ContainerEventAction::Started,
        2_500_000_000,
    );
    let cursor = ContainerEventCursor::beginning().advance(&processed);
    let request = managed_container_events_request("installation-1", &cursor)
        .expect("valid event subscription");

    assert_eq!(request.since.as_deref(), Some("2"));
    assert_eq!(
        request.filters,
        Some(std::collections::HashMap::from([
            ("type".to_owned(), vec!["container".to_owned()]),
            (
                "label".to_owned(),
                vec![
                    "dev.stackctl.managed=true".to_owned(),
                    "dev.stackctl.installation=installation-1".to_owned(),
                ],
            ),
        ]))
    );

    let replayed = EventMessage {
        typ: Some(EventMessageTypeEnum::CONTAINER),
        action: Some("start".to_owned()),
        actor: Some(EventActor {
            id: Some("container-1".to_owned()),
            ..EventActor::default()
        }),
        time_nano: Some(2_500_000_000),
        ..EventMessage::default()
    };
    let new_health_event = EventMessage {
        action: Some("health_status: unhealthy".to_owned()),
        time_nano: Some(2_500_000_001),
        ..replayed.clone()
    };
    let simultaneous_distinct_event = EventMessage {
        action: Some("die".to_owned()),
        ..replayed.clone()
    };

    assert_eq!(
        container_event(replayed, &cursor).expect("valid event"),
        None
    );
    assert!(
        container_event(simultaneous_distinct_event, &cursor)
            .expect("valid event")
            .is_some()
    );
    let event = container_event(new_health_event, &cursor)
        .expect("valid event")
        .expect("new event");
    assert_eq!(event.container_id().as_str(), "container-1");
    assert_eq!(event.action(), &ContainerEventAction::HealthUnhealthy);
    assert_eq!(event.occurred_at_nanoseconds(), 2_500_000_001);
    assert_eq!(cursor.advance(&event).nanoseconds(), 2_500_000_001);
}

#[test]
fn managed_container_events_ignore_exec_activity() {
    let event = EventMessage {
        typ: Some(EventMessageTypeEnum::CONTAINER),
        action: Some("exec_die".to_owned()),
        actor: Some(EventActor {
            id: Some("container-database".to_owned()),
            ..EventActor::default()
        }),
        time_nano: Some(2_500_000_000),
        ..EventMessage::default()
    };

    assert_eq!(
        container_event(event, &ContainerEventCursor::beginning()).expect("valid Engine event"),
        None
    );
}

#[test]
fn managed_container_event_subscriptions_require_an_installation_identity() {
    let error = managed_container_events_request("", &ContainerEventCursor::beginning())
        .expect_err("empty installation identity");

    assert_eq!(
        error.to_string(),
        "managed event installation ID must not be empty"
    );
}

#[test]
fn container_event_source_is_streaming_and_object_safe() {
    let source = RecordingContainerEventSource;
    let strategy: &dyn ContainerEventSource = &source;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let event = runtime
        .block_on(async {
            strategy
                .stream_managed("installation-1", ContainerEventCursor::beginning())
                .next()
                .await
        })
        .expect("event item")
        .expect("valid event");

    assert_eq!(event.action(), &ContainerEventAction::Started);
}

#[test]
fn log_stream_options_are_typed_and_preserve_non_utf8_bytes() {
    let options =
        ContainerLogOptions::new(true, ContainerLogTail::last(25).expect("positive log tail"));
    let request = log_request(&options);
    let chunk = log_chunk(LogOutput::StdErr {
        message: vec![0xff, b'\n'].into(),
    });

    assert!(request.follow);
    assert!(request.stdout);
    assert!(request.stderr);
    assert!(!request.timestamps);
    assert_eq!(request.tail, "25");
    assert!(chunk.is_stderr());
    assert_eq!(chunk.bytes(), &[0xff, b'\n']);
    assert_eq!(
        ContainerLogTail::last(0)
            .expect_err("zero log tail")
            .to_string(),
        "container log tail must be greater than zero"
    );
}

#[test]
fn log_source_requires_an_owned_container_and_is_object_safe() {
    let source = RecordingLogSource;
    let strategy: &dyn LogSource = &source;
    let container = OwnedContainer::new(
        ContainerId::new("container-1"),
        global_metadata(ResourceKind::ProjectApplication),
    );
    let options = ContainerLogOptions::new(false, ContainerLogTail::all());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let chunk = runtime
        .block_on(async {
            let mut stream = strategy.logs(&container, &options).await?;
            stream.next().await.expect("log item")
        })
        .expect("valid log chunk");

    assert_eq!(chunk.bytes(), b"ready\n");
}

#[test]
fn command_requests_map_to_non_privileged_structured_engine_exec() {
    let request = CommandRequest::new(
        vec!["php".to_owned(), "artisan".to_owned(), "migrate".to_owned()],
        BTreeMap::from([
            ("APP_ENV".to_owned(), "local".to_owned()),
            ("DB_PASSWORD".to_owned(), "secret".to_owned()),
        ]),
        Some("/workspace".to_owned()),
    )
    .expect("valid command request");

    let engine_request = command_create_request(&request);

    assert_eq!(
        engine_request.cmd,
        Some(vec![
            "php".to_owned(),
            "artisan".to_owned(),
            "migrate".to_owned(),
        ])
    );
    assert_eq!(
        engine_request.env,
        Some(vec![
            "APP_ENV=local".to_owned(),
            "DB_PASSWORD=secret".to_owned(),
        ])
    );
    assert_eq!(engine_request.working_dir, Some("/workspace".to_owned()));
    assert_eq!(engine_request.attach_stdin, Some(true));
    assert_eq!(engine_request.attach_stdout, Some(true));
    assert_eq!(engine_request.attach_stderr, Some(true));
    assert_eq!(engine_request.tty, Some(false));
    assert_eq!(engine_request.privileged, Some(false));
    assert!(!format!("{request:?}").contains("secret"));
}

#[test]
fn managed_container_environment_maps_to_engine_without_debug_leaks() {
    let options = ContainerCreateOptions::new(
        "stackctl-shared-postgres",
        concat!(
            "postgres@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ),
        global_metadata(ResourceKind::SharedService),
    )
    .expect("container options")
    .with_environment(BTreeMap::from([
        ("POSTGRES_DB".to_owned(), "postgres".to_owned()),
        ("POSTGRES_PASSWORD".to_owned(), "root-secret".to_owned()),
        ("discovery.type".to_owned(), "single-node".to_owned()),
    ]))
    .expect("container environment");

    let (_, request) = create_request(&options);

    assert_eq!(
        request.env,
        Some(vec![
            "POSTGRES_DB=postgres".to_owned(),
            "POSTGRES_PASSWORD=root-secret".to_owned(),
            "discovery.type=single-node".to_owned(),
        ])
    );
    assert!(!format!("{options:?}").contains("root-secret"));
    assert!(format!("{options:?}").contains("POSTGRES_PASSWORD"));
}

#[test]
fn managed_containers_explicitly_prohibit_privilege_escalation() {
    let options = ContainerCreateOptions::new(
        "stackctl-bill-app",
        concat!(
            "dunglas/frankenphp@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ),
        global_metadata(ResourceKind::ProjectApplication),
    )
    .expect("container options");

    let (_, request) = create_request(&options);
    let host = request.host_config.expect("managed host configuration");

    assert_eq!(host.privileged, Some(false));
    assert_eq!(
        host.security_opt,
        Some(vec!["no-new-privileges=true".to_owned()])
    );
}

#[test]
fn managed_named_volumes_remain_distinct_from_host_bind_mounts() {
    let options = ContainerCreateOptions::new(
        "stackctl-shared-postgres",
        concat!(
            "postgres@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ),
        global_metadata(ResourceKind::SharedService),
    )
    .expect("container options")
    .with_volume_mount(
        VolumeMount::read_write("stackctl-postgres-data", "/var/lib/postgresql/data")
            .expect("named volume mount"),
    )
    .with_bind_mount(
        BindMount::read_only("/Users/developer/Stackctl/config", "/etc/stackctl/config")
            .expect("host bind mount"),
    );

    let (_, request) = create_request(&options);
    let host = request.host_config.expect("host config");
    assert_eq!(
        host.binds,
        Some(vec![
            "/Users/developer/Stackctl/config:/etc/stackctl/config:ro".to_owned()
        ])
    );
    let mounts = host.mounts.expect("mounts");

    assert_eq!(mounts.len(), 1);
    assert_eq!(mounts[0].typ, Some(bollard::models::MountType::VOLUME));
    assert_eq!(mounts[0].source.as_deref(), Some("stackctl-postgres-data"));
    assert_eq!(
        mounts[0].target.as_deref(),
        Some("/var/lib/postgresql/data")
    );
    assert_eq!(mounts[0].read_only, Some(false));
}

#[test]
fn managed_shared_memory_is_forwarded_without_host_port_exposure() {
    let options = ContainerCreateOptions::new(
        "stackctl-ephemeral-browser",
        concat!(
            "selenium/standalone-chromium@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ),
        global_metadata(ResourceKind::EphemeralService),
    )
    .expect("container options")
    .with_shared_memory_bytes(2_147_483_648)
    .expect("browser shared memory");

    let (_, request) = create_request(&options);
    let host = request.host_config.expect("host configuration");

    assert_eq!(host.shm_size, Some(2_147_483_648));
    assert_eq!(host.port_bindings, None);
}

#[test]
fn managed_container_platform_is_explicitly_forwarded_to_the_engine() {
    let options = ContainerCreateOptions::new(
        "stackctl-shared-postgres",
        concat!(
            "postgres@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ),
        global_metadata(ResourceKind::SharedService),
    )
    .expect("container options")
    .with_platform("linux/arm64")
    .expect("Linux platform");

    let (query, _) = create_request(&options);

    assert_eq!(query.platform, "linux/arm64");
}

#[test]
fn command_requests_reject_ambiguous_or_unsafe_values_before_engine_access() {
    let empty_command =
        CommandRequest::new(Vec::new(), BTreeMap::new(), None).expect_err("empty command");
    let relative_directory = CommandRequest::new(
        vec!["php".to_owned()],
        BTreeMap::new(),
        Some("workspace".to_owned()),
    )
    .expect_err("relative container directory");
    let invalid_environment = CommandRequest::new(
        vec!["php".to_owned()],
        BTreeMap::from([("BAD=KEY".to_owned(), "value".to_owned())]),
        None,
    )
    .expect_err("invalid environment key");

    assert_eq!(
        empty_command.to_string(),
        "container command must not be empty"
    );
    assert_eq!(
        relative_directory.to_string(),
        "container command working directory 'workspace' must be absolute"
    );
    assert_eq!(
        invalid_environment.to_string(),
        "container command environment key 'BAD=KEY' is invalid"
    );
}

#[test]
fn command_executor_boundary_is_object_safe() {
    fn accepts_command_executor(_executor: &dyn CommandExecutor) {}

    let _ = accepts_command_executor;
}

#[test]
fn health_observations_distinguish_process_and_readiness_states() {
    let stopped = EngineContainerState {
        running: Some(false),
        ..EngineContainerState::default()
    };
    let restarting = EngineContainerState {
        running: Some(true),
        restarting: Some(true),
        ..EngineContainerState::default()
    };
    let unverified = EngineContainerState {
        running: Some(true),
        ..EngineContainerState::default()
    };
    let starting = EngineContainerState {
        running: Some(true),
        health: Some(Health {
            status: Some(HealthStatusEnum::STARTING),
            ..Health::default()
        }),
        ..EngineContainerState::default()
    };
    let healthy = EngineContainerState {
        health: Some(Health {
            status: Some(HealthStatusEnum::HEALTHY),
            ..Health::default()
        }),
        ..starting.clone()
    };
    let unhealthy = EngineContainerState {
        health: Some(Health {
            status: Some(HealthStatusEnum::UNHEALTHY),
            failing_streak: Some(3),
            ..Health::default()
        }),
        ..starting.clone()
    };

    assert_eq!(
        container_health(Some(&stopped)).unwrap(),
        ContainerHealth::Stopped
    );
    assert_eq!(
        container_health(Some(&restarting)).unwrap(),
        ContainerHealth::Restarting
    );
    assert_eq!(
        container_health(Some(&unverified)).unwrap(),
        ContainerHealth::RunningUnverified
    );
    assert_eq!(
        container_health(Some(&starting)).unwrap(),
        ContainerHealth::Starting
    );
    assert_eq!(
        container_health(Some(&healthy)).unwrap(),
        ContainerHealth::Healthy
    );
    assert_eq!(
        container_health(Some(&unhealthy)).unwrap(),
        ContainerHealth::Unhealthy { failing_streak: 3 }
    );
}

#[test]
fn health_observer_boundary_is_object_safe() {
    fn accepts_health_observer(_observer: &dyn HealthObserver) {}

    let _ = accepts_health_observer;
}

#[test]
fn resource_metrics_normalize_idle_benchmark_inputs_without_floats() {
    let stats = ContainerStatsResponse {
        id: Some("container-1".to_owned()),
        cpu_stats: Some(ContainerCpuStats {
            cpu_usage: Some(ContainerCpuUsage {
                total_usage: Some(300),
                ..ContainerCpuUsage::default()
            }),
            system_cpu_usage: Some(1_000),
            online_cpus: Some(2),
            ..ContainerCpuStats::default()
        }),
        precpu_stats: Some(ContainerCpuStats {
            cpu_usage: Some(ContainerCpuUsage {
                total_usage: Some(100),
                ..ContainerCpuUsage::default()
            }),
            system_cpu_usage: Some(500),
            ..ContainerCpuStats::default()
        }),
        memory_stats: Some(ContainerMemoryStats {
            usage: Some(1_024),
            ..ContainerMemoryStats::default()
        }),
        pids_stats: Some(ContainerPidsStats {
            current: Some(3),
            ..ContainerPidsStats::default()
        }),
        networks: Some(std::collections::HashMap::from([
            (
                "eth0".to_owned(),
                ContainerNetworkStats {
                    rx_bytes: Some(10),
                    tx_bytes: Some(5),
                    ..ContainerNetworkStats::default()
                },
            ),
            (
                "eth1".to_owned(),
                ContainerNetworkStats {
                    rx_bytes: Some(20),
                    tx_bytes: Some(7),
                    ..ContainerNetworkStats::default()
                },
            ),
        ])),
        ..ContainerStatsResponse::default()
    };

    let metrics = container_resource_metrics(&stats, &ContainerId::new("container-1"))
        .expect("complete metrics");

    assert_eq!(
        metrics,
        ContainerResourceMetrics::new(Some(8_000), Some(1_024), Some(3), Some(30), Some(12))
    );
}

#[test]
fn resource_metrics_reject_stats_for_a_different_container() {
    let stats = ContainerStatsResponse {
        id: Some("foreign-container".to_owned()),
        ..ContainerStatsResponse::default()
    };

    let error = container_resource_metrics(&stats, &ContainerId::new("container-1"))
        .expect_err("foreign stats");

    assert_eq!(
        error.to_string(),
        "Engine stats belong to container 'foreign-container', expected 'container-1'"
    );
}

#[test]
fn resource_metrics_boundary_is_object_safe() {
    fn accepts_resource_metrics(_metrics: &dyn ResourceMetrics) {}

    let _ = accepts_resource_metrics;
}

#[test]
fn image_build_requests_are_content_addressed_labeled_and_offline() {
    let metadata = global_metadata(ResourceKind::Build);
    let dockerfile = concat!(
        "FROM ghcr.io/stackctl/php@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
        "COPY . /workspace\n"
    );
    let request = ImageBuildRequest::new(
        BTreeMap::from([("runtime.json".to_owned(), br#"{"php":"8.4"}"#.to_vec())]),
        "Dockerfile".to_owned(),
        dockerfile.to_owned(),
        "linux/arm64".to_owned(),
        metadata.clone(),
    )
    .expect("valid immutable build");

    let options = build_image_options(&request);
    assert_eq!(options.dockerfile, "Dockerfile");
    assert_eq!(options.t.as_deref(), Some(request.output_tag()));
    assert_eq!(options.networkmode.as_deref(), Some("none"));
    assert_eq!(options.pull.as_deref(), Some("false"));
    assert_eq!(options.remote, None);
    assert!(options.rm);
    assert!(options.forcerm);
    assert_eq!(options.platform, "linux/arm64");
    assert_eq!(options.labels, None);
    assert!(request.output_tag().starts_with("stackctl-build:"));
    let dockerfile_with_labels = request.dockerfile_contents();
    assert_eq!(dockerfile_with_labels.matches("\nLABEL ").count(), 1);
    assert!(dockerfile_with_labels.contains(&format!(
        "dev.stackctl.build-input=\"{}\"",
        request.input_digest()
    )));
    for (key, value) in metadata.labels() {
        assert!(
            dockerfile_with_labels.contains(&format!(
                "{key}={}",
                serde_json::to_string(&value).expect("encode expected image label")
            )),
            "Dockerfile is missing managed image label '{key}'"
        );
    }
    let mut files = BTreeMap::new();
    for entry in tar::Archive::new(request.context_tar())
        .entries()
        .expect("build context entries")
    {
        let mut entry = entry.expect("build context entry");
        let path = entry
            .path()
            .expect("build context path")
            .to_string_lossy()
            .into_owned();
        let mut contents = Vec::new();
        entry
            .read_to_end(&mut contents)
            .expect("build context contents");
        files.insert(path, contents);
    }
    assert_eq!(files["Dockerfile"], dockerfile_with_labels.as_bytes());
    assert_eq!(files["runtime.json"], br#"{"php":"8.4"}"#);
    assert!(!format!("{request:?}").contains("runtime.json"));
}

#[test]
fn networked_image_build_requests_use_the_engine_build_network() {
    let metadata = global_metadata(ResourceKind::Build);
    let dockerfile = concat!(
        "FROM dunglas/frankenphp@sha256:",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
        "RUN [\"install-php-extensions\",\"soap\"]\n"
    );
    let request = ImageBuildRequest::new_networked(
        BTreeMap::new(),
        "Dockerfile".to_owned(),
        dockerfile.to_owned(),
        "linux/arm64".to_owned(),
        metadata,
    )
    .expect("valid networked build");

    let options = build_image_options(&request);

    assert_eq!(options.networkmode, None);
}

#[test]
fn image_build_requests_reject_mutable_bases_and_remote_additions() {
    let mutable_base = ImageBuildRequest::new(
        BTreeMap::new(),
        "Dockerfile".to_owned(),
        "FROM php:8.4\n".to_owned(),
        "linux/amd64".to_owned(),
        global_metadata(ResourceKind::Build),
    )
    .expect_err("mutable base image");
    let remote_add = ImageBuildRequest::new(
        BTreeMap::new(),
        "Dockerfile".to_owned(),
        concat!(
            "FROM php@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n",
            "ADD https://example.test/installer.sh /tmp/installer.sh\n"
        )
        .to_owned(),
        "linux/amd64".to_owned(),
        global_metadata(ResourceKind::Build),
    )
    .expect_err("remote ADD");
    let unsafe_context = ImageBuildRequest::new(
        BTreeMap::from([("../secret".to_owned(), Vec::new())]),
        "Dockerfile".to_owned(),
        concat!(
            "FROM php@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"
        )
        .to_owned(),
        "linux/amd64".to_owned(),
        global_metadata(ResourceKind::Build),
    )
    .expect_err("unsafe context path");
    let shadowed_dockerfile = ImageBuildRequest::new(
        BTreeMap::from([("Dockerfile".to_owned(), b"FROM scratch\n".to_vec())]),
        "Dockerfile".to_owned(),
        "FROM scratch\n".to_owned(),
        "linux/amd64".to_owned(),
        global_metadata(ResourceKind::Build),
    )
    .expect_err("shadowed Dockerfile");

    assert_eq!(
        mutable_base.to_string(),
        "image build base 'php:8.4' must use an immutable sha256 digest"
    );
    assert_eq!(
        remote_add.to_string(),
        "image build Dockerfile must not ADD remote URLs"
    );
    assert_eq!(
        unsafe_context.to_string(),
        "image build context path '../secret' must be a safe relative archive path"
    );
    assert_eq!(
        shadowed_dockerfile.to_string(),
        "image build context must not define reserved Dockerfile path 'Dockerfile'"
    );
}

#[test]
fn image_builder_boundary_is_object_safe() {
    fn accepts_image_builder(_builder: &dyn ImageBuilder) {}

    let _ = accepts_image_builder;
}

#[test]
fn bollard_request_maps_only_typed_values_and_reserved_labels() {
    let metadata = global_metadata(ResourceKind::Gateway);
    let options = ContainerCreateOptions::new(
        "stackctl-gateway",
        concat!(
            "ghcr.io/stackctl/gateway@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ),
        metadata,
    )
    .expect("immutable container options");

    let (query, body) = create_request(&options);

    assert_eq!(query.name.as_deref(), Some("stackctl-gateway"));
    assert_eq!(body.image.as_deref(), Some(options.image()));
    assert_eq!(
        body.labels.expect("ownership labels"),
        options.metadata().labels().into_iter().collect()
    );
}

#[test]
fn published_port_discovery_uses_unfiltered_running_container_inventory() {
    let request = published_port_list_request();

    assert!(!request.all);
    assert_eq!(request.filters, None);
    assert!(!request.size);
}

#[test]
fn published_port_discovery_preserves_engine_owner_and_tcp_bindings() {
    let bindings = published_port_bindings(ContainerSummary {
        id: Some("container-1".to_owned()),
        names: Some(vec!["/legacy-proxy".to_owned()]),
        ports: Some(vec![
            PortSummary {
                ip: Some("127.0.0.1".to_owned()),
                private_port: 8080,
                public_port: Some(80),
                typ: Some(PortSummaryTypeEnum::TCP),
            },
            PortSummary {
                ip: Some("127.0.0.1".to_owned()),
                private_port: 5353,
                public_port: Some(53),
                typ: Some(PortSummaryTypeEnum::UDP),
            },
            PortSummary {
                ip: None,
                private_port: 8443,
                public_port: Some(443),
                typ: Some(PortSummaryTypeEnum::TCP),
            },
        ]),
        ..ContainerSummary::default()
    })
    .expect("published ports");

    assert_eq!(
        bindings,
        vec![
            PublishedPortBinding::new(
                "container-1",
                "legacy-proxy",
                "127.0.0.1".parse().expect("host IP"),
                80,
            )
            .expect("binding"),
            PublishedPortBinding::new(
                "container-1",
                "legacy-proxy",
                "0.0.0.0".parse().expect("unspecified host IP"),
                443,
            )
            .expect("all-interface binding"),
        ]
    );
}

#[test]
fn published_port_discovery_rejects_invalid_engine_host_addresses() {
    let error = published_port_bindings(ContainerSummary {
        id: Some("container-1".to_owned()),
        names: Some(vec!["/legacy-proxy".to_owned()]),
        ports: Some(vec![PortSummary {
            ip: Some("not-an-ip".to_owned()),
            private_port: 8080,
            public_port: Some(80),
            typ: Some(PortSummaryTypeEnum::TCP),
        }]),
        ..ContainerSummary::default()
    })
    .expect_err("invalid Engine host address");

    assert!(
        error.to_string().starts_with(
            "Engine returned invalid host IP 'not-an-ip' for container 'legacy-proxy':"
        )
    );
}

#[test]
fn published_port_discovery_is_an_object_safe_engine_capability() {
    fn accepts_published_ports(_source: &dyn PublishedPortDiscovery) {}

    let _ = accepts_published_ports;
}

#[test]
fn gateway_engine_request_has_private_network_loopback_ports_and_read_only_tls() {
    let metadata = global_metadata(ResourceKind::Gateway);
    let options = gateway_container_request(GatewayContainerRequestOptions::new(
        concat!(
            "ghcr.io/stackctl/gateway@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )
        .to_owned(),
        "stackctl".to_owned(),
        "501:20".to_owned(),
        std::path::PathBuf::from("/state/tls/wildcard.crt"),
        std::path::PathBuf::from("/state/tls/wildcard.key"),
        std::path::PathBuf::from("/state/gateway/config.json"),
        metadata,
    ))
    .expect("immutable gateway options");

    let (_, body) = create_request(&options);
    assert_eq!(body.user.as_deref(), Some("501:20"));
    let host = body.host_config.expect("gateway host config");

    assert_eq!(host.network_mode.as_deref(), Some("stackctl"));
    let port_bindings = host.port_bindings.expect("loopback port bindings");
    assert_eq!(
        port_bindings
            .keys()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from(["443/tcp".to_owned(), "80/tcp".to_owned()])
    );
    assert!(
        port_bindings
            .values()
            .filter_map(Option::as_ref)
            .flatten()
            .all(|binding| binding
                .host_port
                .as_deref()
                .is_some_and(|port| { port == "80" || port == "443" }))
    );
    assert_eq!(
        port_bindings
            .values()
            .filter_map(Option::as_ref)
            .flatten()
            .filter_map(|binding| binding.host_ip.clone())
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from(["127.0.0.1".to_owned(), "::1".to_owned()])
    );
    let binds = host.binds.expect("gateway binds");
    assert!(
        binds.contains(&"/state/tls/wildcard.crt:/etc/stackctl/tls/wildcard.crt:ro".to_owned())
    );
    assert!(
        binds.contains(&"/state/tls/wildcard.key:/etc/stackctl/tls/wildcard.key:ro".to_owned())
    );
    assert!(binds.iter().all(|mount| !mount.contains("/ca.key:")));
    assert!(binds.contains(&"/state/gateway/config.json:/etc/stackctl/config.json:ro".to_owned()));
    assert_eq!(
        host.tmpfs.expect("gateway ephemeral filesystems"),
        std::collections::HashMap::from([
            (
                "/config".to_owned(),
                "rw,noexec,nosuid,size=16777216".to_owned(),
            ),
            (
                "/data".to_owned(),
                "rw,noexec,nosuid,size=16777216".to_owned(),
            ),
            (
                "/tmp".to_owned(),
                "rw,noexec,nosuid,size=16777216".to_owned(),
            ),
        ])
    );
    assert_eq!(
        body.cmd,
        Some(vec![
            "caddy".to_owned(),
            "run".to_owned(),
            "--config".to_owned(),
            "/etc/stackctl/config.json".to_owned(),
        ])
    );
    let health_check = body.healthcheck.expect("gateway health check");
    assert_eq!(
        health_check.test,
        Some(vec![
            "CMD".to_owned(),
            "caddy".to_owned(),
            "validate".to_owned(),
            "--config".to_owned(),
            "/etc/stackctl/config.json".to_owned(),
        ])
    );
    assert_eq!(health_check.interval, Some(30_000_000_000));
    assert_eq!(health_check.timeout, Some(5_000_000_000));
    assert_eq!(health_check.start_period, Some(10_000_000_000));
    assert_eq!(health_check.retries, Some(3));
    assert_eq!(health_check.start_interval, None);
    assert_eq!(
        host.restart_policy.expect("restart policy").name,
        Some(bollard::models::RestartPolicyNameEnum::UNLESS_STOPPED)
    );
}

#[test]
fn container_health_checks_reject_invalid_or_unbounded_settings() {
    let empty = ContainerHealthCheck::new(
        Vec::new(),
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_secs(1),
        1,
    )
    .expect_err("empty health command");
    let short_interval = ContainerHealthCheck::new(
        vec!["health".to_owned()],
        Duration::from_nanos(999_999),
        Duration::from_secs(1),
        Duration::from_secs(1),
        1,
    )
    .expect_err("sub-millisecond interval");
    let no_retries = ContainerHealthCheck::new(
        vec!["health".to_owned()],
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_secs(1),
        0,
    )
    .expect_err("zero retries");
    let excessive_timeout = ContainerHealthCheck::new(
        vec!["health".to_owned()],
        Duration::from_secs(1),
        Duration::MAX,
        Duration::from_secs(1),
        1,
    )
    .expect_err("unrepresentable Engine timeout");

    assert_eq!(
        empty.to_string(),
        "container health check must contain a non-empty executable and no NUL bytes"
    );
    assert_eq!(
        short_interval.to_string(),
        "container health check interval must be at least 1 millisecond"
    );
    assert_eq!(
        no_retries.to_string(),
        "container health check retries must be greater than zero"
    );
    assert_eq!(
        excessive_timeout.to_string(),
        "container health check timeout exceeds the Engine duration limit"
    );
}

#[test]
fn managed_containers_can_disable_an_inherited_image_health_check() {
    let options = ContainerCreateOptions::new(
        "stackctl-bill-worker",
        concat!(
            "dunglas/frankenphp@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ),
        global_metadata(ResourceKind::ProjectProcess),
    )
    .expect("container options")
    .without_image_health_check();

    let (_, request) = create_request(&options);

    assert_eq!(
        request.healthcheck.expect("disabled health check").test,
        Some(vec!["NONE".to_owned()])
    );
}

#[test]
fn attached_commands_track_the_canonical_container_id() {
    assert_eq!(
        canonical_command_container_id(Some("sha256-container-id"))
            .expect("canonical container ID")
            .as_str(),
        "sha256-container-id"
    );
    assert!(canonical_command_container_id(None).is_err());
}

#[test]
fn network_management_is_an_object_safe_owned_resource_strategy() {
    let options = NetworkCreateOptions::new("stackctl", global_metadata(ResourceKind::Network))
        .expect("managed network");
    let mut backend = RecordingNetworkBackend::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let network = runtime
        .block_on(create_network_through_strategy(&mut backend, &options))
        .expect("create network");
    runtime
        .block_on(backend.remove_network(&network))
        .expect("remove proven-owned network");

    assert_eq!(network.id().as_str(), "network-1");
    assert_eq!(backend.created, vec![options]);
    assert_eq!(backend.removed, vec![network]);
}

#[test]
fn network_requests_use_bridge_driver_and_complete_ownership_labels() {
    let metadata = global_metadata(ResourceKind::Network);
    let options = NetworkCreateOptions::new("stackctl", metadata.clone()).expect("managed network");

    let request = network_create_request(&options);

    assert_eq!(request.name, "stackctl");
    assert_eq!(request.driver.as_deref(), Some("bridge"));
    assert_eq!(
        request.labels.expect("network ownership labels"),
        metadata.labels().into_iter().collect()
    );
}

#[test]
fn network_deletion_rejects_missing_or_changed_ownership_labels() {
    let network = OwnedNetwork::new(
        NetworkId::new("network-1"),
        global_metadata(ResourceKind::Network),
    );

    let error = verify_owned_network_labels(&network, &std::collections::HashMap::new())
        .expect_err("unlabelled network");

    assert_eq!(
        error.to_string(),
        "refusing to delete network 'network-1' because its Engine ownership labels no longer match"
    );
}

#[test]
fn volume_management_requires_owned_volume_proof_for_deletion() {
    let options = VolumeCreateOptions::new(
        "stackctl-postgres-17-data",
        project_metadata(ResourceKind::Volume),
    )
    .expect("managed volume");
    let mut backend = RecordingVolumeBackend::default();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let volume = runtime
        .block_on(create_volume_through_strategy(&mut backend, &options))
        .expect("create volume");
    runtime
        .block_on(backend.remove_volume(&volume))
        .expect("remove proven-owned volume");

    assert_eq!(volume.name(), "stackctl-postgres-17-data");
    assert_eq!(backend.created, vec![options]);
    assert_eq!(backend.removed, vec![volume]);
}

#[test]
fn volume_requests_are_deterministic_local_volumes_with_complete_labels() {
    let metadata = project_metadata(ResourceKind::Volume);
    let options = VolumeCreateOptions::new("stackctl-postgres-17-data", metadata.clone())
        .expect("managed volume");

    let request = volume_create_request(&options);

    assert_eq!(request.name.as_deref(), Some("stackctl-postgres-17-data"));
    assert_eq!(request.driver.as_deref(), Some("local"));
    assert_eq!(
        request.labels.expect("volume ownership labels"),
        metadata.labels().into_iter().collect()
    );
}

#[test]
fn volume_deletion_rejects_missing_or_changed_ownership_labels() {
    let metadata = project_metadata(ResourceKind::Volume);
    let volume = OwnedVolume::new("stackctl-postgres-17-data", metadata);

    let error = verify_owned_volume_labels(&volume, &std::collections::HashMap::new())
        .expect_err("unlabelled volume");

    assert_eq!(
        error.to_string(),
        "refusing to delete volume 'stackctl-postgres-17-data' because its Engine ownership labels no longer match"
    );
}

#[test]
fn volume_archive_requires_exact_container_ownership_and_mount() {
    let container_metadata = project_metadata(ResourceKind::ProjectService)
        .with_resource_id("search")
        .expect("service identity");
    let volume_metadata = project_metadata(ResourceKind::Volume)
        .with_resource_id("search")
        .expect("volume identity");
    let container = OwnedContainer::new(ContainerId::new("search-container"), container_metadata);
    let volume = OwnedVolume::new("stackctl-bill-search-data", volume_metadata);
    let mounts = vec![MountPoint {
        typ: Some("volume".to_owned()),
        name: Some("stackctl-bill-search-data".to_owned()),
        destination: Some("/var/lib/search".to_owned()),
        ..MountPoint::default()
    }];

    validate_volume_archive_identity(&container, &volume).expect("exact archive ownership");
    assert_eq!(
        exact_volume_mount_target(&mounts, volume.name()).expect("exact named-volume mount"),
        "/var/lib/search"
    );
}

#[test]
fn volume_archive_rejects_unrelated_or_ambiguous_mounts() {
    let container = OwnedContainer::new(
        ContainerId::new("search-container"),
        project_metadata(ResourceKind::ProjectService)
            .with_resource_id("search")
            .expect("service identity"),
    );
    let unrelated = OwnedVolume::new(
        "stackctl-bill-cache-data",
        project_metadata(ResourceKind::Volume)
            .with_resource_id("cache")
            .expect("volume identity"),
    );
    let identity_error = validate_volume_archive_identity(&container, &unrelated)
        .expect_err("unrelated volume must not be archived through this container");
    assert!(
        identity_error
            .to_string()
            .contains("exact archive ownership")
    );

    let duplicate_mounts = vec![
        MountPoint {
            typ: Some("volume".to_owned()),
            name: Some("stackctl-bill-search-data".to_owned()),
            destination: Some("/data-a".to_owned()),
            ..MountPoint::default()
        },
        MountPoint {
            typ: Some("volume".to_owned()),
            name: Some("stackctl-bill-search-data".to_owned()),
            destination: Some("/data-b".to_owned()),
            ..MountPoint::default()
        },
    ];
    let mount_error = exact_volume_mount_target(&duplicate_mounts, "stackctl-bill-search-data")
        .expect_err("ambiguous volume mounts must fail closed");
    assert!(mount_error.to_string().contains("exactly one"));
}

#[test]
fn volume_archive_upload_uses_mount_parent() {
    assert_eq!(
        volume_archive_upload_target("/data").expect("top-level mount target"),
        "/"
    );
    assert_eq!(
        volume_archive_upload_target("/var/lib/search").expect("nested mount target"),
        "/var/lib"
    );
    assert!(
        volume_archive_upload_target("/").is_err(),
        "root cannot be an owned volume mount"
    );
    assert!(
        volume_archive_upload_target("relative/data").is_err(),
        "relative mount cannot be an archive target"
    );
}

#[test]
fn volume_subpath_archive_stays_inside_the_exact_owned_mount() {
    assert_eq!(
        volume_archive_subpath_target(
            "/var/lib/rabbitmq",
            "mnesia/rabbit@localhost/msg_stores/vhosts/628Q7P"
        )
        .expect("safe vhost message-store path"),
        "/var/lib/rabbitmq/mnesia/rabbit@localhost/msg_stores/vhosts/628Q7P"
    );
    for unsafe_path in ["", ".", "../other-volume", "/etc", "data/../../etc"] {
        assert!(
            volume_archive_subpath_target("/var/lib/rabbitmq", unsafe_path).is_err(),
            "unsafe volume subpath '{unsafe_path}' must fail closed"
        );
    }
    assert_eq!(
        volume_archive_subpath_upload_target(
            "/var/lib/rabbitmq",
            "mnesia/rabbit@localhost/msg_stores/vhosts/628Q7P"
        )
        .expect("safe vhost message-store upload parent"),
        "/var/lib/rabbitmq/mnesia/rabbit@localhost/msg_stores/vhosts"
    );
}

#[test]
fn empty_desired_revision_is_rejected_before_resource_creation() {
    let error = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::Gateway,
        project_id: None,
        compatibility_fingerprint: "gateway-v1".to_owned(),
        schema_version: 8,
        desired_revision: String::new(),
        retention: RetentionClass::Disposable,
    })
    .expect_err("empty desired revision");

    assert_eq!(
        error.to_string(),
        "managed resource desired revision must not be empty"
    );
}

#[test]
fn complete_current_installation_labels_reconstruct_owned_metadata() {
    let metadata = project_metadata(ResourceKind::ProjectApplication)
        .with_compatibility_profile("php", "8.4")
        .expect("compatibility profile");

    let ownership = classify_observed_resource(&metadata.labels(), "install-1", 8);

    assert_eq!(
        ownership,
        ObservedResourceOwnership::Owned(Box::new(metadata))
    );
}

#[test]
fn managed_container_observations_reconstruct_mutable_owned_handles() {
    let metadata = project_metadata(ResourceKind::ProjectApplication);
    let observed = ObservedContainer::new(ContainerId::new("container-1"), metadata.labels());

    let owned = reconstruct_owned_container(&observed, "install-1", 8)
        .expect("current installation container");

    assert_eq!(owned.id().as_str(), "container-1");
    assert_eq!(owned.metadata(), &metadata);
}

#[test]
fn unmanaged_container_observations_never_reconstruct_mutable_handles() {
    let observed = ObservedContainer::new(ContainerId::new("container-1"), BTreeMap::new());

    let ownership =
        reconstruct_owned_container(&observed, "install-1", 8).expect_err("unmanaged container");

    assert_eq!(ownership, ObservedResourceOwnership::Unmanaged);
}

#[test]
fn unlabelled_resources_are_never_adopted() {
    let ownership = classify_observed_resource(&BTreeMap::new(), "install-1", 8);

    assert_eq!(ownership, ObservedResourceOwnership::Unmanaged);
}

#[test]
fn foreign_installation_resources_are_never_adopted() {
    let labels = project_metadata(ResourceKind::ProjectApplication).labels();

    let ownership = classify_observed_resource(&labels, "install-2", 8);

    assert_eq!(
        ownership,
        ObservedResourceOwnership::ForeignInstallation {
            installation_id: "install-1".to_owned(),
        }
    );
}

#[test]
fn incomplete_managed_labels_are_not_treated_as_owned() {
    let mut labels = project_metadata(ResourceKind::ProjectApplication).labels();
    labels.remove("dev.stackctl.desired");

    let ownership = classify_observed_resource(&labels, "install-1", 8);

    assert_eq!(
        ownership,
        ObservedResourceOwnership::Malformed {
            detail: "managed resource label 'dev.stackctl.desired' is missing".to_owned(),
        }
    );
}

#[test]
fn unsupported_resource_schema_is_not_treated_as_owned() {
    let labels = project_metadata(ResourceKind::ProjectApplication).labels();

    let ownership = classify_observed_resource(&labels, "install-1", 9);

    assert_eq!(
        ownership,
        ObservedResourceOwnership::UnsupportedSchema {
            found: 8,
            supported: 9,
        }
    );
}

#[test]
fn managed_container_rescan_includes_stopped_objects_and_filters_by_marker() {
    let request = managed_container_list_request();

    assert!(request.all);
    assert_eq!(
        request.filters,
        Some(std::collections::HashMap::from([(
            "label".to_owned(),
            vec!["dev.stackctl.managed=true".to_owned()],
        )]))
    );
}

#[test]
fn managed_image_network_and_volume_rescans_filter_by_the_reserved_marker() {
    let image_request = managed_image_list_request();
    let network_request = managed_network_list_request();
    let volume_request = managed_volume_list_request();
    let expected = Some(std::collections::HashMap::from([(
        "label".to_owned(),
        vec!["dev.stackctl.managed=true".to_owned()],
    )]));

    assert!(image_request.all);
    assert_eq!(image_request.filters, expected);
    assert_eq!(network_request.filters, expected);
    assert_eq!(volume_request.filters, expected);
}

#[test]
fn image_observations_reconstruct_owned_handles_from_labels() {
    let metadata = build_cache_metadata();
    let id = format!("sha256:{}", "c".repeat(64));
    let observed = observed_image(ImageSummary {
        id: id.clone(),
        labels: metadata.labels().into_iter().collect(),
        ..ImageSummary::default()
    })
    .expect("observed image");

    let owned = reconstruct_owned_image(&observed, "install-1", 8).expect("owned image");

    assert_eq!(owned.id().as_str(), id);
    assert_eq!(owned.metadata(), &metadata);
}

#[test]
fn network_and_volume_discovery_are_narrow_object_safe_strategies() {
    let network = ObservedNetwork::new(
        NetworkId::new("network-1"),
        global_metadata(ResourceKind::Network).labels(),
    );
    let volume = ObservedVolume::new(
        "stackctl-postgres-17-data",
        project_metadata(ResourceKind::Volume).labels(),
    );
    let backend = RecordingResourceDiscovery {
        networks: vec![network.clone()],
        volumes: vec![volume.clone()],
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");
    let network_discovery: &dyn NetworkDiscovery = &backend;
    let volume_discovery: &dyn VolumeDiscovery = &backend;

    assert_eq!(
        runtime
            .block_on(network_discovery.discover_managed_networks())
            .expect("discover networks"),
        vec![network]
    );
    assert_eq!(
        runtime
            .block_on(volume_discovery.discover_managed_volumes())
            .expect("discover volumes"),
        vec![volume]
    );
}

#[test]
fn network_and_volume_observations_reconstruct_owned_handles_from_labels() {
    let network_metadata = global_metadata(ResourceKind::Network);
    let volume_metadata = project_metadata(ResourceKind::Volume);
    let network = observed_network(Network {
        id: Some("network-1".to_owned()),
        labels: Some(network_metadata.labels().into_iter().collect()),
        ..Network::default()
    })
    .expect("observed network");
    let volume = observed_volume(Volume {
        name: "stackctl-postgres-17-data".to_owned(),
        labels: volume_metadata.labels().into_iter().collect(),
        ..Volume::default()
    })
    .expect("observed volume");

    let owned_network = reconstruct_owned_network(&network, "install-1", 8).expect("owned network");
    let owned_volume = reconstruct_owned_volume(&volume, "install-1", 8).expect("owned volume");

    assert_eq!(owned_network.id().as_str(), "network-1");
    assert_eq!(owned_network.metadata(), &network_metadata);
    assert_eq!(owned_volume.name(), "stackctl-postgres-17-data");
    assert_eq!(owned_volume.metadata(), &volume_metadata);
}

#[test]
fn engine_container_summaries_map_to_backend_independent_observations() {
    let labels = project_metadata(ResourceKind::ProjectApplication)
        .labels()
        .into_iter()
        .collect();
    let summary = ContainerSummary {
        id: Some("container-1".to_owned()),
        labels: Some(labels),
        ..ContainerSummary::default()
    };

    let observed = observed_container(summary).expect("complete Engine summary");

    assert_eq!(observed.id().as_str(), "container-1");
    assert_eq!(
        classify_observed_resource(observed.labels(), "install-1", 8),
        ObservedResourceOwnership::Owned(Box::new(project_metadata(
            ResourceKind::ProjectApplication
        )))
    );
}

#[test]
fn engine_summaries_without_ids_fail_instead_of_disappearing() {
    let error = observed_container(ContainerSummary::default()).expect_err("missing ID");

    assert_eq!(
        error.to_string(),
        "Engine returned a managed container without an ID"
    );
}

#[test]
fn container_discovery_is_an_object_safe_rescan_strategy() {
    let mut backend = RecordingContainerBackend::default();
    backend.observed.push(ObservedContainer::new(
        ContainerId::new("container-1"),
        project_metadata(ResourceKind::ProjectApplication).labels(),
    ));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let observed = runtime
        .block_on(discover_through_strategy(&backend))
        .expect("discover containers");

    assert_eq!(observed, backend.observed);
}

#[test]
fn minimum_supported_engine_api_version_is_accepted() {
    validate_engine_api_version(ClientVersion {
        major_version: 1,
        minor_version: 41,
    })
    .expect("minimum supported API");
}

#[test]
fn older_engine_api_versions_fail_before_reconciliation() {
    let error = validate_engine_api_version(ClientVersion {
        major_version: 1,
        minor_version: 40,
    })
    .expect_err("unsupported Engine API");

    assert_eq!(
        error.to_string(),
        "Engine API version 1.40 is unsupported; version 1.41 or newer is required"
    );
}

#[test]
fn installation_cleanup_deletes_only_exact_owned_resources_in_dependency_order() {
    let container_metadata = global_metadata(ResourceKind::Gateway);
    let volume_metadata = global_metadata(ResourceKind::Volume);
    let network_metadata = global_metadata(ResourceKind::Network);
    let image_metadata = build_cache_metadata();
    let mut foreign_labels = container_metadata.labels();
    foreign_labels.insert(
        "dev.stackctl.installation".to_owned(),
        "install-2".to_owned(),
    );
    let mut backend = RecordingContainerBackend {
        observed: vec![
            ObservedContainer::new(
                ContainerId::new("owned-container"),
                container_metadata.labels(),
            ),
            ObservedContainer::new(ContainerId::new("foreign-container"), foreign_labels),
        ],
        observed_volumes: vec![ObservedVolume::new(
            "owned-volume",
            volume_metadata.labels(),
        )],
        observed_networks: vec![ObservedNetwork::new(
            NetworkId::new("owned-network"),
            network_metadata.labels(),
        )],
        observed_images: vec![ObservedImage::new(
            ImageId::new(format!("sha256:{}", "a".repeat(64))).expect("image ID"),
            100,
            0,
            image_metadata.labels(),
        )],
        ..RecordingContainerBackend::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    runtime
        .block_on(delete_owned_installation_resources(
            &mut backend,
            InstallationResourceDeletionOptions {
                installation_id: "install-1",
                schema_version: 8,
                authorized_persistent_volumes: &[],
            },
        ))
        .expect("owned installation cleanup");

    assert_eq!(
        backend.removals,
        [
            "stop:owned-container",
            "container:owned-container",
            &format!("image:sha256:{}", "a".repeat(64)),
            "volume:owned-volume",
            "network:owned-network",
        ]
    );
}

#[test]
fn installation_cleanup_refuses_non_build_cache_images_before_mutation() {
    let mut backend = RecordingContainerBackend {
        observed_images: vec![ObservedImage::new(
            ImageId::new(format!("sha256:{}", "b".repeat(64))).expect("image ID"),
            100,
            0,
            global_metadata(ResourceKind::Gateway).labels(),
        )],
        ..RecordingContainerBackend::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(delete_owned_installation_resources(
            &mut backend,
            InstallationResourceDeletionOptions {
                installation_id: "install-1",
                schema_version: 8,
                authorized_persistent_volumes: &[],
            },
        ))
        .expect_err("non-build image must fail closed");

    assert!(error.to_string().contains("resource kind"));
    assert!(backend.removals.is_empty());
}

#[test]
fn installation_cleanup_refuses_ambiguous_owned_labels_before_mutation() {
    let mut labels = global_metadata(ResourceKind::Gateway).labels();
    labels.insert("dev.stackctl.schema".to_owned(), "7".to_owned());
    let mut backend = RecordingContainerBackend {
        observed: vec![ObservedContainer::new(
            ContainerId::new("ambiguous-container"),
            labels,
        )],
        ..RecordingContainerBackend::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(delete_owned_installation_resources(
            &mut backend,
            InstallationResourceDeletionOptions {
                installation_id: "install-1",
                schema_version: 8,
                authorized_persistent_volumes: &[],
            },
        ))
        .expect_err("ambiguous ownership must fail closed");

    assert!(error.to_string().contains("UnsupportedSchema"));
    assert!(backend.removals.is_empty());
}

#[test]
fn installation_cleanup_refuses_unprotected_observed_project_volumes() {
    let mut backend = RecordingContainerBackend {
        observed_volumes: vec![ObservedVolume::new(
            "stackctl-bill-search-data",
            project_metadata(ResourceKind::Volume).labels(),
        )],
        ..RecordingContainerBackend::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(delete_owned_installation_resources(
            &mut backend,
            InstallationResourceDeletionOptions {
                installation_id: "install-1",
                schema_version: 8,
                authorized_persistent_volumes: &[],
            },
        ))
        .expect_err("unprotected observed project volume must fail closed");

    assert!(error.to_string().contains("stackctl-bill-search-data"));
    assert!(error.to_string().contains("exact recovery authorization"));
    assert!(backend.removals.is_empty());
}

#[test]
fn installation_cleanup_deletes_only_exact_authorized_project_volume() {
    let volume_name = "stackctl-bill-search-data".to_owned();
    let mut backend = RecordingContainerBackend {
        observed_volumes: vec![ObservedVolume::new(
            &volume_name,
            project_metadata(ResourceKind::Volume).labels(),
        )],
        ..RecordingContainerBackend::default()
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("test runtime");

    runtime
        .block_on(delete_owned_installation_resources(
            &mut backend,
            InstallationResourceDeletionOptions {
                installation_id: "install-1",
                schema_version: 8,
                authorized_persistent_volumes: std::slice::from_ref(&volume_name),
            },
        ))
        .expect("authorized project volume cleanup");

    assert_eq!(backend.removals, ["volume:stackctl-bill-search-data"]);
}

#[test]
fn engine_operation_deadlines_return_structured_timeouts() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("test runtime");

    let error = runtime
        .block_on(bounded_engine_operation(
            "inspect container",
            Duration::from_millis(1),
            pending::<Result<(), EngineError>>(),
        ))
        .expect_err("operation deadline");

    assert_eq!(
        error,
        EngineError::Timeout {
            action: "inspect container".to_owned(),
            timeout_milliseconds: 1,
        }
    );
    assert_eq!(
        error.to_string(),
        "Engine operation 'inspect container' timed out after 1 ms"
    );
}

fn project_metadata(kind: ResourceKind) -> ManagedResourceMetadata {
    ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind,
        project_id: Some("bill".to_owned()),
        compatibility_fingerprint: "runtime-v1".to_owned(),
        schema_version: 8,
        desired_revision: "desired-v1".to_owned(),
        retention: RetentionClass::Persistent,
    })
    .expect("valid project metadata")
}

fn global_metadata(kind: ResourceKind) -> ManagedResourceMetadata {
    ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind,
        project_id: None,
        compatibility_fingerprint: "gateway-v1".to_owned(),
        schema_version: 8,
        desired_revision: "desired-v1".to_owned(),
        retention: RetentionClass::Disposable,
    })
    .expect("valid global metadata")
}

fn build_cache_metadata() -> ManagedResourceMetadata {
    ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
        installation_id: "install-1".to_owned(),
        kind: ResourceKind::Build,
        project_id: Some("bill".to_owned()),
        compatibility_fingerprint: "runtime-v1".to_owned(),
        schema_version: 8,
        desired_revision: "desired-v1".to_owned(),
        retention: RetentionClass::BuildCache,
    })
    .expect("valid build-cache metadata")
}

fn create_through_strategy<'operation>(
    strategy: &'operation mut dyn ContainerLifecycle,
    options: &'operation ContainerCreateOptions,
) -> EngineFuture<'operation, OwnedContainer> {
    strategy.create(options)
}

fn discover_through_strategy(
    strategy: &dyn ContainerDiscovery,
) -> EngineFuture<'_, Vec<ObservedContainer>> {
    strategy.discover_managed()
}

fn create_network_through_strategy<'operation>(
    strategy: &'operation mut dyn NetworkManager,
    options: &'operation NetworkCreateOptions,
) -> EngineFuture<'operation, OwnedNetwork> {
    strategy.create_network(options)
}

fn create_volume_through_strategy<'operation>(
    strategy: &'operation mut dyn VolumeManager,
    options: &'operation VolumeCreateOptions,
) -> EngineFuture<'operation, OwnedVolume> {
    strategy.create_volume(options)
}

#[derive(Default)]
struct RecordingVolumeBackend {
    created: Vec<VolumeCreateOptions>,
    removed: Vec<OwnedVolume>,
}

struct RecordingResourceDiscovery {
    networks: Vec<ObservedNetwork>,
    volumes: Vec<ObservedVolume>,
}

#[derive(Default)]
struct RecordingImageResolver {
    resolved: Vec<ImmutableImageReference>,
}

#[derive(Default)]
struct RecordingImageReferenceResolver {
    resolved: Vec<RegistryImageReference>,
}

struct RecordingContainerEventSource;

impl ContainerEventSource for RecordingContainerEventSource {
    fn stream_managed<'stream>(
        &'stream self,
        _installation_id: &'stream str,
        _cursor: ContainerEventCursor,
    ) -> ContainerEventStream<'stream> {
        Box::pin(futures_util::stream::once(async {
            Ok(ContainerEvent::new(
                ContainerId::new("container-1"),
                ContainerEventAction::Started,
                1,
            ))
        }))
    }
}

struct RecordingLogSource;

struct RecordingContainerCompletion;

impl ContainerCompletion for RecordingContainerCompletion {
    fn wait_for_success<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
        _timeout: Duration,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }
}

impl LogSource for RecordingLogSource {
    fn logs<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
        _options: &'operation ContainerLogOptions,
    ) -> EngineFuture<'operation, ContainerLogStream<'operation>> {
        Box::pin(async {
            let stream: ContainerLogStream<'operation> =
                Box::pin(futures_util::stream::once(async {
                    Ok(LogChunk::stdout(b"ready\n".to_vec()))
                }));

            Ok(stream)
        })
    }
}

impl ImageResolver for RecordingImageResolver {
    fn ensure_image<'operation>(
        &'operation mut self,
        reference: &'operation ImmutableImageReference,
    ) -> EngineFuture<'operation, ImageId> {
        Box::pin(async move {
            self.resolved.push(reference.clone());
            ImageId::new(format!("sha256:{}", "a".repeat(64)))
        })
    }
}

impl ImageReferenceResolver for RecordingImageReferenceResolver {
    fn resolve_image_reference<'operation>(
        &'operation mut self,
        reference: &'operation RegistryImageReference,
    ) -> EngineFuture<'operation, ImmutableImageReference> {
        Box::pin(async move {
            self.resolved.push(reference.clone());
            reference.with_digest(concat!(
                "sha256:",
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            ))
        })
    }
}

impl NetworkDiscovery for RecordingResourceDiscovery {
    fn discover_managed_networks(&self) -> EngineFuture<'_, Vec<ObservedNetwork>> {
        Box::pin(async { Ok(self.networks.clone()) })
    }
}

impl VolumeDiscovery for RecordingResourceDiscovery {
    fn discover_managed_volumes(&self) -> EngineFuture<'_, Vec<ObservedVolume>> {
        Box::pin(async { Ok(self.volumes.clone()) })
    }
}

impl VolumeManager for RecordingVolumeBackend {
    fn create_volume<'operation>(
        &'operation mut self,
        options: &'operation VolumeCreateOptions,
    ) -> EngineFuture<'operation, OwnedVolume> {
        Box::pin(async move {
            self.created.push(options.clone());
            Ok(OwnedVolume::new(options.name(), options.metadata().clone()))
        })
    }

    fn remove_volume<'operation>(
        &'operation mut self,
        volume: &'operation OwnedVolume,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.removed.push(volume.clone());
            Ok(())
        })
    }
}

#[derive(Default)]
struct RecordingNetworkBackend {
    created: Vec<NetworkCreateOptions>,
    removed: Vec<OwnedNetwork>,
}

impl NetworkManager for RecordingNetworkBackend {
    fn create_network<'operation>(
        &'operation mut self,
        options: &'operation NetworkCreateOptions,
    ) -> EngineFuture<'operation, OwnedNetwork> {
        Box::pin(async move {
            self.created.push(options.clone());
            Ok(OwnedNetwork::new(
                NetworkId::new("network-1"),
                options.metadata().clone(),
            ))
        })
    }

    fn remove_network<'operation>(
        &'operation mut self,
        network: &'operation OwnedNetwork,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.removed.push(network.clone());
            Ok(())
        })
    }
}

#[derive(Default)]
struct RecordingContainerBackend {
    created: Vec<ContainerCreateOptions>,
    observed: Vec<ObservedContainer>,
    observed_networks: Vec<ObservedNetwork>,
    observed_volumes: Vec<ObservedVolume>,
    observed_images: Vec<ObservedImage>,
    removals: Vec<String>,
}

impl ImageDiscovery for RecordingContainerBackend {
    fn discover_managed_images(&self) -> EngineFuture<'_, Vec<ObservedImage>> {
        Box::pin(async { Ok(self.observed_images.clone()) })
    }
}

impl ImageManager for RecordingContainerBackend {
    fn remove_image<'operation>(
        &'operation mut self,
        image: &'operation OwnedImage,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.removals.push(format!("image:{}", image.id().as_str()));
            Ok(())
        })
    }
}

impl ContainerDiscovery for RecordingContainerBackend {
    fn discover_managed(&self) -> EngineFuture<'_, Vec<ObservedContainer>> {
        Box::pin(async { Ok(self.observed.clone()) })
    }
}

impl ContainerLifecycle for RecordingContainerBackend {
    fn create<'operation>(
        &'operation mut self,
        options: &'operation ContainerCreateOptions,
    ) -> EngineFuture<'operation, OwnedContainer> {
        Box::pin(async move {
            self.created.push(options.clone());
            Ok(OwnedContainer::new(
                ContainerId::new("container-1"),
                options.metadata().clone(),
            ))
        })
    }

    fn start<'operation>(
        &'operation mut self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async { Ok(()) })
    }

    fn stop<'operation>(
        &'operation mut self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.removals
                .push(format!("stop:{}", container.id().as_str()));
            Ok(())
        })
    }

    fn remove<'operation>(
        &'operation mut self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.removals
                .push(format!("container:{}", container.id().as_str()));
            Ok(())
        })
    }

    fn inspect<'operation>(
        &'operation self,
        _container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ContainerState> {
        Box::pin(async { Ok(ContainerState::Running) })
    }
}

impl VolumeDiscovery for RecordingContainerBackend {
    fn discover_managed_volumes(&self) -> EngineFuture<'_, Vec<ObservedVolume>> {
        Box::pin(async { Ok(self.observed_volumes.clone()) })
    }
}

impl VolumeManager for RecordingContainerBackend {
    fn create_volume<'operation>(
        &'operation mut self,
        _options: &'operation VolumeCreateOptions,
    ) -> EngineFuture<'operation, OwnedVolume> {
        Box::pin(async {
            Err(EngineError::Backend {
                detail: "unexpected volume creation".to_owned(),
            })
        })
    }

    fn remove_volume<'operation>(
        &'operation mut self,
        volume: &'operation OwnedVolume,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.removals.push(format!("volume:{}", volume.name()));
            Ok(())
        })
    }
}

impl NetworkDiscovery for RecordingContainerBackend {
    fn discover_managed_networks(&self) -> EngineFuture<'_, Vec<ObservedNetwork>> {
        Box::pin(async { Ok(self.observed_networks.clone()) })
    }
}

impl NetworkManager for RecordingContainerBackend {
    fn create_network<'operation>(
        &'operation mut self,
        _options: &'operation NetworkCreateOptions,
    ) -> EngineFuture<'operation, OwnedNetwork> {
        Box::pin(async {
            Err(EngineError::Backend {
                detail: "unexpected network creation".to_owned(),
            })
        })
    }

    fn remove_network<'operation>(
        &'operation mut self,
        network: &'operation OwnedNetwork,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.removals
                .push(format!("network:{}", network.id().as_str()));
            Ok(())
        })
    }
}
