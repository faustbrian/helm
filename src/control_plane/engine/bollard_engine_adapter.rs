use super::bounded_engine_operation::bounded_engine_operation;
use super::{
    ContainerCreateOptions, ContainerDiscovery, ContainerId, ContainerLifecycle, ContainerState,
    EngineError, EngineFuture, ImageId, ImageResolver, ImmutableImageReference,
    NetworkCreateOptions, NetworkDiscovery, NetworkId, NetworkManager, ObservedContainer,
    ObservedNetwork, ObservedResourceOwnership, ObservedVolume, OwnedContainer, OwnedNetwork,
    OwnedVolume, VolumeCreateOptions, VolumeDiscovery, VolumeManager, classify_observed_resource,
};
use bollard::errors::Error as BollardError;
use bollard::models::{
    ContainerCreateBody, ContainerSummary, HostConfig, Mount, MountType, Network,
    NetworkCreateRequest, PortBinding, RestartPolicy, RestartPolicyNameEnum, Volume,
    VolumeCreateRequest,
};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, CreateImageOptionsBuilder, ListContainersOptionsBuilder,
    ListNetworksOptionsBuilder, ListVolumesOptionsBuilder, RemoveVolumeOptions,
};
use bollard::{API_DEFAULT_VERSION, ClientVersion, Docker};
use futures_util::TryStreamExt;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::time::Duration;

const REQUEST_TIMEOUT_SECONDS: u64 = 120;
const MINIMUM_ENGINE_API_VERSION: ClientVersion = ClientVersion {
    major_version: 1,
    minor_version: 41,
};

/// A direct Docker-compatible Engine API adapter used for Docker and Podman.
pub(crate) struct BollardEngineAdapter {
    docker: Docker,
}

impl BollardEngineAdapter {
    /// Connects directly to a Docker-compatible Unix socket.
    #[cfg(unix)]
    pub(crate) async fn connect_unix(socket_path: &Path) -> Result<Self, EngineError> {
        let socket_path = socket_path
            .to_str()
            .ok_or_else(|| EngineError::InvalidRequest {
                detail: format!(
                    "Engine socket path '{}' is not valid UTF-8",
                    socket_path.display()
                ),
            })?;
        let docker =
            Docker::connect_with_unix(socket_path, REQUEST_TIMEOUT_SECONDS, API_DEFAULT_VERSION)
                .map_err(|error| backend_error("connect to Engine socket", error))?;

        let docker = negotiate_engine_api(docker).await?;

        Ok(Self { docker })
    }

    /// Connects directly to a Docker-compatible Windows named pipe.
    #[cfg(windows)]
    pub(crate) async fn connect_named_pipe(pipe: &str) -> Result<Self, EngineError> {
        let docker =
            Docker::connect_with_named_pipe(pipe, REQUEST_TIMEOUT_SECONDS, API_DEFAULT_VERSION)
                .map_err(|error| backend_error("connect to Engine named pipe", error))?;

        let docker = negotiate_engine_api(docker).await?;

        Ok(Self { docker })
    }
}

impl ContainerLifecycle for BollardEngineAdapter {
    fn create<'operation>(
        &'operation mut self,
        options: &'operation ContainerCreateOptions,
    ) -> EngineFuture<'operation, OwnedContainer> {
        Box::pin(async move {
            let (query, body) = create_request(options);
            let response = bounded_engine_operation("create container", request_timeout(), async {
                self.docker
                    .create_container(Some(query), body)
                    .await
                    .map_err(|error| backend_error("create container", error))
            })
            .await?;

            Ok(OwnedContainer::new(
                ContainerId::new(response.id),
                options.metadata().clone(),
            ))
        })
    }

    fn start<'operation>(
        &'operation mut self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            bounded_engine_operation("start container", request_timeout(), async {
                let observed = self
                    .docker
                    .inspect_container(container.id().as_str(), None)
                    .await
                    .map_err(|error| backend_error("verify container ownership", error))?;
                verify_owned_container_labels(
                    container,
                    &observed
                        .config
                        .and_then(|config| config.labels)
                        .unwrap_or_default(),
                )?;

                self.docker
                    .start_container(container.id().as_str(), None)
                    .await
                    .map_err(|error| backend_error("start container", error))
            })
            .await
        })
    }

    fn stop<'operation>(
        &'operation mut self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            bounded_engine_operation("stop container", request_timeout(), async {
                let observed = self
                    .docker
                    .inspect_container(container.id().as_str(), None)
                    .await
                    .map_err(|error| backend_error("verify container ownership", error))?;
                verify_owned_container_labels(
                    container,
                    &observed
                        .config
                        .and_then(|config| config.labels)
                        .unwrap_or_default(),
                )?;

                self.docker
                    .stop_container(container.id().as_str(), None)
                    .await
                    .map_err(|error| backend_error("stop container", error))
            })
            .await
        })
    }

    fn remove<'operation>(
        &'operation mut self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            bounded_engine_operation("remove container", request_timeout(), async {
                let observed = self
                    .docker
                    .inspect_container(container.id().as_str(), None)
                    .await
                    .map_err(|error| backend_error("verify container ownership", error))?;
                verify_owned_container_labels(
                    container,
                    &observed
                        .config
                        .and_then(|config| config.labels)
                        .unwrap_or_default(),
                )?;

                self.docker
                    .remove_container(container.id().as_str(), None)
                    .await
                    .map_err(|error| backend_error("remove container", error))
            })
            .await
        })
    }

    fn inspect<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ContainerState> {
        Box::pin(async move {
            bounded_engine_operation("inspect container", request_timeout(), async {
                match self
                    .docker
                    .inspect_container(container.id().as_str(), None)
                    .await
                {
                    Ok(response) => {
                        let running = response
                            .state
                            .and_then(|state| state.running)
                            .unwrap_or(false);

                        if running {
                            Ok(ContainerState::Running)
                        } else {
                            Ok(ContainerState::Stopped)
                        }
                    }
                    Err(BollardError::DockerResponseServerError {
                        status_code: 404, ..
                    }) => Ok(ContainerState::Missing),
                    Err(error) => Err(backend_error("inspect container", error)),
                }
            })
            .await
        })
    }
}

impl ContainerDiscovery for BollardEngineAdapter {
    fn discover_managed(&self) -> EngineFuture<'_, Vec<ObservedContainer>> {
        Box::pin(async move {
            bounded_engine_operation("list managed containers", request_timeout(), async {
                self.docker
                    .list_containers(Some(managed_container_list_request()))
                    .await
                    .map_err(|error| backend_error("list managed containers", error))
            })
            .await?
            .into_iter()
            .map(observed_container)
            .collect()
        })
    }
}

impl ImageResolver for BollardEngineAdapter {
    fn ensure_image<'operation>(
        &'operation mut self,
        reference: &'operation ImmutableImageReference,
    ) -> EngineFuture<'operation, ImageId> {
        Box::pin(async move {
            bounded_engine_operation("ensure immutable image", request_timeout(), async {
                match self.docker.inspect_image(reference.as_str()).await {
                    Ok(image) => image_id(image.id, reference),
                    Err(BollardError::DockerResponseServerError {
                        status_code: 404, ..
                    }) => {
                        self.docker
                            .create_image(Some(image_pull_request(reference)), None, None)
                            .try_collect::<Vec<_>>()
                            .await
                            .map_err(|error| backend_error("pull immutable image", error))?;
                        let image = self
                            .docker
                            .inspect_image(reference.as_str())
                            .await
                            .map_err(|error| backend_error("inspect pulled image", error))?;

                        image_id(image.id, reference)
                    }
                    Err(error) => Err(backend_error("inspect immutable image", error)),
                }
            })
            .await
        })
    }
}

pub(super) fn image_pull_request(
    reference: &ImmutableImageReference,
) -> bollard::query_parameters::CreateImageOptions {
    CreateImageOptionsBuilder::default()
        .from_image(reference.as_str())
        .build()
}

fn image_id(
    id: Option<String>,
    reference: &ImmutableImageReference,
) -> Result<ImageId, EngineError> {
    id.map(ImageId::new).ok_or_else(|| EngineError::Backend {
        detail: format!(
            "Engine returned immutable image '{}' without an ID",
            reference.as_str()
        ),
    })
}

impl NetworkDiscovery for BollardEngineAdapter {
    fn discover_managed_networks(&self) -> EngineFuture<'_, Vec<ObservedNetwork>> {
        Box::pin(async move {
            bounded_engine_operation("list managed networks", request_timeout(), async {
                self.docker
                    .list_networks(Some(managed_network_list_request()))
                    .await
                    .map_err(|error| backend_error("list managed networks", error))
            })
            .await?
            .into_iter()
            .map(observed_network)
            .collect()
        })
    }
}

impl VolumeDiscovery for BollardEngineAdapter {
    fn discover_managed_volumes(&self) -> EngineFuture<'_, Vec<ObservedVolume>> {
        Box::pin(async move {
            let response =
                bounded_engine_operation("list managed volumes", request_timeout(), async {
                    self.docker
                        .list_volumes(Some(managed_volume_list_request()))
                        .await
                        .map_err(|error| backend_error("list managed volumes", error))
                })
                .await?;

            response
                .volumes
                .unwrap_or_default()
                .into_iter()
                .map(observed_volume)
                .collect()
        })
    }
}

pub(super) fn verify_owned_container_labels(
    container: &OwnedContainer,
    labels: &HashMap<String, String>,
) -> Result<(), EngineError> {
    if labels_match_metadata(labels, container.metadata()) {
        return Ok(());
    }

    Err(EngineError::OwnershipMismatch {
        action: "mutate",
        resource_kind: "container",
        resource_id: container.id().as_str().to_owned(),
    })
}

impl NetworkManager for BollardEngineAdapter {
    fn create_network<'operation>(
        &'operation mut self,
        options: &'operation NetworkCreateOptions,
    ) -> EngineFuture<'operation, OwnedNetwork> {
        Box::pin(async move {
            let request = network_create_request(options);
            let response = bounded_engine_operation("create network", request_timeout(), async {
                self.docker
                    .create_network(request)
                    .await
                    .map_err(|error| backend_error("create network", error))
            })
            .await?;

            Ok(OwnedNetwork::new(
                NetworkId::new(response.id),
                options.metadata().clone(),
            ))
        })
    }

    fn remove_network<'operation>(
        &'operation mut self,
        network: &'operation OwnedNetwork,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            bounded_engine_operation("remove network", request_timeout(), async {
                let observed = self
                    .docker
                    .inspect_network(network.id().as_str(), None)
                    .await
                    .map_err(|error| backend_error("verify network ownership", error))?;
                verify_owned_network_labels(network, &observed.labels.unwrap_or_default())?;

                self.docker
                    .remove_network(network.id().as_str())
                    .await
                    .map_err(|error| backend_error("remove network", error))
            })
            .await
        })
    }
}

pub(super) fn verify_owned_network_labels(
    network: &OwnedNetwork,
    labels: &HashMap<String, String>,
) -> Result<(), EngineError> {
    if labels_match_metadata(labels, network.metadata()) {
        return Ok(());
    }

    Err(EngineError::OwnershipMismatch {
        action: "delete",
        resource_kind: "network",
        resource_id: network.id().as_str().to_owned(),
    })
}

impl VolumeManager for BollardEngineAdapter {
    fn create_volume<'operation>(
        &'operation mut self,
        options: &'operation VolumeCreateOptions,
    ) -> EngineFuture<'operation, OwnedVolume> {
        Box::pin(async move {
            let volume = bounded_engine_operation("create volume", request_timeout(), async {
                self.docker
                    .create_volume(volume_create_request(options))
                    .await
                    .map_err(|error| backend_error("create volume", error))
            })
            .await?;

            Ok(OwnedVolume::new(volume.name, options.metadata().clone()))
        })
    }

    fn remove_volume<'operation>(
        &'operation mut self,
        volume: &'operation OwnedVolume,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            bounded_engine_operation("remove owned volume", request_timeout(), async {
                let observed = self
                    .docker
                    .inspect_volume(volume.name())
                    .await
                    .map_err(|error| backend_error("verify volume ownership", error))?;
                verify_owned_volume_labels(volume, &observed.labels)?;

                self.docker
                    .remove_volume(volume.name(), None::<RemoveVolumeOptions>)
                    .await
                    .map_err(|error| backend_error("remove owned volume", error))
            })
            .await
        })
    }
}

pub(super) fn verify_owned_volume_labels(
    volume: &OwnedVolume,
    labels: &HashMap<String, String>,
) -> Result<(), EngineError> {
    if labels_match_metadata(labels, volume.metadata()) {
        return Ok(());
    }

    Err(EngineError::OwnershipMismatch {
        action: "delete",
        resource_kind: "volume",
        resource_id: volume.name().to_owned(),
    })
}

fn labels_match_metadata(
    labels: &HashMap<String, String>,
    metadata: &super::ManagedResourceMetadata,
) -> bool {
    let labels = labels
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<BTreeMap<_, _>>();
    let ownership = classify_observed_resource(
        &labels,
        metadata.installation_id(),
        metadata.schema_version(),
    );

    ownership == ObservedResourceOwnership::Owned(metadata.clone())
}

pub(super) fn volume_create_request(options: &VolumeCreateOptions) -> VolumeCreateRequest {
    VolumeCreateRequest {
        name: Some(options.name().to_owned()),
        driver: Some("local".to_owned()),
        labels: Some(options.metadata().labels().into_iter().collect()),
        ..VolumeCreateRequest::default()
    }
}

pub(super) fn network_create_request(options: &NetworkCreateOptions) -> NetworkCreateRequest {
    NetworkCreateRequest {
        name: options.name().to_owned(),
        driver: Some("bridge".to_owned()),
        labels: Some(options.metadata().labels().into_iter().collect()),
        ..NetworkCreateRequest::default()
    }
}

pub(super) fn create_request(
    options: &ContainerCreateOptions,
) -> (
    bollard::query_parameters::CreateContainerOptions,
    ContainerCreateBody,
) {
    let query = CreateContainerOptionsBuilder::default()
        .name(options.name())
        .build();
    let body = ContainerCreateBody {
        image: Some(options.image().to_owned()),
        labels: Some(
            options
                .metadata()
                .labels()
                .into_iter()
                .collect::<HashMap<_, _>>(),
        ),
        exposed_ports: (!options.port_bindings().is_empty()).then(|| {
            options
                .port_bindings()
                .iter()
                .map(|binding| format!("{}/tcp", binding.container_port()))
                .collect()
        }),
        host_config: Some(host_config(options)),
        ..ContainerCreateBody::default()
    };

    (query, body)
}

fn host_config(options: &ContainerCreateOptions) -> HostConfig {
    let mut port_bindings = HashMap::new();
    for binding in options.port_bindings() {
        let bindings = port_bindings
            .entry(format!("{}/tcp", binding.container_port()))
            .or_insert_with(|| Some(Vec::new()))
            .as_mut()
            .expect("new port binding list");
        bindings.push(PortBinding {
            host_ip: Some("127.0.0.1".to_owned()),
            host_port: Some(binding.host_port().to_string()),
        });
        bindings.push(PortBinding {
            host_ip: Some("::1".to_owned()),
            host_port: Some(binding.host_port().to_string()),
        });
    }

    HostConfig {
        network_mode: options.network().map(str::to_owned),
        port_bindings: (!port_bindings.is_empty()).then_some(port_bindings),
        mounts: (!options.bind_mounts().is_empty()).then(|| {
            options
                .bind_mounts()
                .iter()
                .map(|mount| Mount {
                    source: Some(mount.source().to_owned()),
                    target: Some(mount.target().to_owned()),
                    typ: Some(MountType::BIND),
                    read_only: Some(mount.is_read_only()),
                    ..Mount::default()
                })
                .collect()
        }),
        restart_policy: options.restart_policy().map(|policy| match policy {
            super::ContainerRestartPolicy::UnlessStopped => RestartPolicy {
                name: Some(RestartPolicyNameEnum::UNLESS_STOPPED),
                maximum_retry_count: None,
            },
        }),
        ..HostConfig::default()
    }
}

pub(super) fn managed_container_list_request() -> bollard::query_parameters::ListContainersOptions {
    let filters = managed_resource_filters();

    ListContainersOptionsBuilder::default()
        .all(true)
        .filters(&filters)
        .build()
}

pub(super) fn managed_network_list_request() -> bollard::query_parameters::ListNetworksOptions {
    let filters = managed_resource_filters();

    ListNetworksOptionsBuilder::default()
        .filters(&filters)
        .build()
}

pub(super) fn managed_volume_list_request() -> bollard::query_parameters::ListVolumesOptions {
    let filters = managed_resource_filters();

    ListVolumesOptionsBuilder::default()
        .filters(&filters)
        .build()
}

fn managed_resource_filters() -> HashMap<String, Vec<String>> {
    HashMap::from([(
        "label".to_owned(),
        vec!["dev.stackctl.managed=true".to_owned()],
    )])
}

pub(super) fn observed_container(
    summary: ContainerSummary,
) -> Result<ObservedContainer, EngineError> {
    let id = summary.id.ok_or_else(|| EngineError::Backend {
        detail: "Engine returned a managed container without an ID".to_owned(),
    })?;
    let labels = summary
        .labels
        .unwrap_or_default()
        .into_iter()
        .collect::<BTreeMap<_, _>>();

    Ok(ObservedContainer::new(ContainerId::new(id), labels))
}

pub(super) fn observed_network(network: Network) -> Result<ObservedNetwork, EngineError> {
    let id = network.id.ok_or_else(|| EngineError::Backend {
        detail: "Engine returned a managed network without an ID".to_owned(),
    })?;
    let labels = network
        .labels
        .unwrap_or_default()
        .into_iter()
        .collect::<BTreeMap<_, _>>();

    Ok(ObservedNetwork::new(NetworkId::new(id), labels))
}

pub(super) fn observed_volume(volume: Volume) -> Result<ObservedVolume, EngineError> {
    if volume.name.is_empty() {
        return Err(EngineError::Backend {
            detail: "Engine returned a managed volume without a name".to_owned(),
        });
    }
    let labels = volume.labels.into_iter().collect::<BTreeMap<_, _>>();

    Ok(ObservedVolume::new(volume.name, labels))
}

async fn negotiate_engine_api(docker: Docker) -> Result<Docker, EngineError> {
    let docker =
        bounded_engine_operation("negotiate Engine API version", request_timeout(), async {
            docker
                .negotiate_version()
                .await
                .map_err(|error| backend_error("negotiate Engine API version", error))
        })
        .await?;

    validate_engine_api_version(docker.client_version())?;

    Ok(docker)
}

const fn request_timeout() -> Duration {
    Duration::from_secs(REQUEST_TIMEOUT_SECONDS)
}

pub(super) fn validate_engine_api_version(version: ClientVersion) -> Result<(), EngineError> {
    if version < MINIMUM_ENGINE_API_VERSION {
        return Err(EngineError::Backend {
            detail: format!(
                "Engine API version {version} is unsupported; version {} or newer is required",
                MINIMUM_ENGINE_API_VERSION
            ),
        });
    }

    Ok(())
}

fn backend_error(action: &str, error: BollardError) -> EngineError {
    EngineError::Backend {
        detail: format!("failed to {action}: {error}"),
    }
}
