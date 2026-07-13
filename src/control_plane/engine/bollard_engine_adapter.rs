use super::{
    ContainerCreateOptions, ContainerId, ContainerLifecycle, ContainerState, EngineError,
    EngineFuture,
};
use bollard::errors::Error as BollardError;
use bollard::models::ContainerCreateBody;
use bollard::query_parameters::CreateContainerOptionsBuilder;
use bollard::{API_DEFAULT_VERSION, Docker};
use std::collections::HashMap;
use std::path::Path;

const REQUEST_TIMEOUT_SECONDS: u64 = 120;

/// A direct Docker-compatible Engine API adapter used for Docker and Podman.
pub(crate) struct BollardEngineAdapter {
    docker: Docker,
}

impl BollardEngineAdapter {
    /// Connects directly to a Docker-compatible Unix socket.
    #[cfg(unix)]
    pub(crate) fn connect_unix(socket_path: &Path) -> Result<Self, EngineError> {
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

        Ok(Self { docker })
    }

    /// Connects directly to a Docker-compatible Windows named pipe.
    #[cfg(windows)]
    pub(crate) fn connect_named_pipe(pipe: &str) -> Result<Self, EngineError> {
        let docker =
            Docker::connect_with_named_pipe(pipe, REQUEST_TIMEOUT_SECONDS, API_DEFAULT_VERSION)
                .map_err(|error| backend_error("connect to Engine named pipe", error))?;

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
            let response = self
                .docker
                .create_container(Some(query), body)
                .await
                .map_err(|error| backend_error("create container", error))?;

            Ok(ContainerId::new(response.id))
        })
    }

    fn start<'operation>(
        &'operation mut self,
        container: &'operation ContainerId,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.docker
                .start_container(container.as_str(), None)
                .await
                .map_err(|error| backend_error("start container", error))
        })
    }

    fn stop<'operation>(
        &'operation mut self,
        container: &'operation ContainerId,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.docker
                .stop_container(container.as_str(), None)
                .await
                .map_err(|error| backend_error("stop container", error))
        })
    }

    fn remove<'operation>(
        &'operation mut self,
        container: &'operation ContainerId,
    ) -> EngineFuture<'operation, ()> {
        Box::pin(async move {
            self.docker
                .remove_container(container.as_str(), None)
                .await
                .map_err(|error| backend_error("remove container", error))
        })
    }

    fn inspect<'operation>(
        &'operation self,
        container: &'operation ContainerId,
    ) -> EngineFuture<'operation, ContainerState> {
        Box::pin(async move {
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

fn backend_error(action: &str, error: BollardError) -> EngineError {
    EngineError::Backend {
        detail: format!("failed to {action}: {error}"),
    }
}
