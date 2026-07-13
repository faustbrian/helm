use super::bounded_engine_operation::bounded_engine_operation;
use super::{
    ContainerCreateOptions, ContainerDiscovery, ContainerId, ContainerLifecycle, ContainerState,
    EngineError, EngineFuture, NetworkCreateOptions, NetworkId, NetworkManager, ObservedContainer,
    ObservedResourceOwnership, OwnedVolume, VolumeCreateOptions, VolumeManager,
    classify_observed_resource,
};
use bollard::errors::Error as BollardError;
use bollard::models::{
    ContainerCreateBody, ContainerSummary, HostConfig, Mount, MountType, NetworkCreateRequest,
    PortBinding, RestartPolicy, RestartPolicyNameEnum, VolumeCreateRequest,
};
use bollard::query_parameters::{
    CreateContainerOptionsBuilder, ListContainersOptionsBuilder, RemoveVolumeOptions,
};
use bollard::{API_DEFAULT_VERSION, ClientVersion, Docker};
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
    ) -> EngineFuture<'operation, ContainerId> {
        Box::pin(async move {
            let (query, body) = create_request(options);
            let response = bounded_engine_operation("create container", request_timeout(), async {
                self.docker
                    .create_container(Some(query), body)
                    .await
                    .map_err(|error| backend_error("create container", error))
            })
            .await?;

            Ok(ContainerId::new(response.id))
        })
    }

    fn start<'operation>(
        &'operation mut self,
        container: &'operation ContainerId,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            bounded_engine_operation("start container", request_timeout(), async {
                self.docker
                    .start_container(container.as_str(), None)
                    .await
                    .map_err(|error| backend_error("start container", error))
            })
            .await
        })
    }

    fn stop<'operation>(
        &'operation mut self,
        container: &'operation ContainerId,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            bounded_engine_operation("stop container", request_timeout(), async {
                self.docker
                    .stop_container(container.as_str(), None)
                    .await
                    .map_err(|error| backend_error("stop container", error))
            })
            .await
        })
    }

    fn remove<'operation>(
        &'operation mut self,
        container: &'operation ContainerId,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            bounded_engine_operation("remove container", request_timeout(), async {
                self.docker
                    .remove_container(container.as_str(), None)
                    .await
                    .map_err(|error| backend_error("remove container", error))
            })
            .await
        })
    }

    fn inspect<'operation>(
        &'operation self,
        container: &'operation ContainerId,
    ) -> EngineFuture<'operation, ContainerState> {
        Box::pin(async move {
            bounded_engine_operation("inspect container", request_timeout(), async {
                match self
                    .docker
                    .inspect_container(container.as_str(), None)
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

impl NetworkManager for BollardEngineAdapter {
    fn create_network<'operation>(
        &'operation mut self,
        options: &'operation NetworkCreateOptions,
    ) -> EngineFuture<'operation, NetworkId> {
        Box::pin(async move {
            let request = network_create_request(options);
            let response = bounded_engine_operation("create network", request_timeout(), async {
                self.docker
                    .create_network(request)
                    .await
                    .map_err(|error| backend_error("create network", error))
            })
            .await?;

            Ok(NetworkId::new(response.id))
        })
    }

    fn remove_network<'operation>(
        &'operation mut self,
        network: &'operation NetworkId,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            bounded_engine_operation("remove network", request_timeout(), async {
                self.docker
                    .remove_network(network.as_str())
                    .await
                    .map_err(|error| backend_error("remove network", error))
            })
            .await
        })
    }
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
    let labels = labels
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<BTreeMap<_, _>>();
    let metadata = volume.metadata();
    let ownership = classify_observed_resource(
        &labels,
        metadata.installation_id(),
        metadata.schema_version(),
    );

    if ownership == ObservedResourceOwnership::Owned(metadata.clone()) {
        return Ok(());
    }

    Err(EngineError::OwnershipMismatch {
        resource_kind: "volume",
        resource_id: volume.name().to_owned(),
    })
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
    let filters = HashMap::from([(
        "label".to_owned(),
        vec!["dev.stackctl.managed=true".to_owned()],
    )]);

    ListContainersOptionsBuilder::default()
        .all(true)
        .filters(&filters)
        .build()
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
