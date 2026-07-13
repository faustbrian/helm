use super::bounded_engine_operation::bounded_engine_operation;
use super::{
    ContainerCreateOptions, ContainerDiscovery, ContainerId, ContainerLifecycle, ContainerState,
    EngineError, EngineFuture, ObservedContainer,
};
use bollard::errors::Error as BollardError;
use bollard::models::{ContainerCreateBody, ContainerSummary};
use bollard::query_parameters::{CreateContainerOptionsBuilder, ListContainersOptionsBuilder};
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
        ..ContainerCreateBody::default()
    };

    (query, body)
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
