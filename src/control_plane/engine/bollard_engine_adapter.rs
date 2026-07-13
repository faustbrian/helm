use super::bounded_engine_operation::bounded_engine_operation;
use super::managed_resource_metadata::{INSTALLATION_LABEL, MANAGED_LABEL};
use super::{
    CommandExecutionId, CommandExecutor, CommandRequest, CommandSession, CommandStatus,
    ContainerCreateOptions, ContainerDiscovery, ContainerEvent, ContainerEventAction,
    ContainerEventCursor, ContainerEventSource, ContainerEventStream, ContainerHealth, ContainerId,
    ContainerLifecycle, ContainerLogOptions, ContainerLogStream, ContainerResourceMetrics,
    ContainerState, EngineError, EngineFuture, HealthObserver, ImageBuildRequest, ImageBuilder,
    ImageId, ImageResolver, ImmutableImageReference, LogChunk, LogSource, LogStreamKind,
    NetworkCreateOptions, NetworkDiscovery, NetworkId, NetworkManager, ObservedContainer,
    ObservedNetwork, ObservedResourceOwnership, ObservedVolume, OwnedContainer, OwnedNetwork,
    OwnedVolume, ResourceMetrics, VolumeCreateOptions, VolumeDiscovery, VolumeManager,
    classify_observed_resource,
};
use bollard::container::LogOutput;
use bollard::errors::Error as BollardError;
use bollard::exec::{CreateExecOptions, StartExecOptions, StartExecResults};
use bollard::models::{
    ContainerCpuStats, ContainerCreateBody, ContainerNetworkStats,
    ContainerState as EngineContainerState, ContainerStatsResponse, ContainerSummary, EventMessage,
    EventMessageTypeEnum, HealthStatusEnum, HostConfig, Mount, MountType, Network,
    NetworkCreateRequest, PortBinding, RestartPolicy, RestartPolicyNameEnum, Volume,
    VolumeCreateRequest,
};
use bollard::query_parameters::{
    BuildImageOptions, BuildImageOptionsBuilder, CreateContainerOptionsBuilder,
    CreateImageOptionsBuilder, EventsOptions, EventsOptionsBuilder, ListContainersOptionsBuilder,
    ListNetworksOptionsBuilder, ListVolumesOptionsBuilder, LogsOptions, LogsOptionsBuilder,
    RemoveVolumeOptions, StatsOptionsBuilder,
};
use bollard::{API_DEFAULT_VERSION, ClientVersion, Docker, body_full};
use futures_util::{StreamExt, TryStreamExt};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::time::Duration;

const REQUEST_TIMEOUT_SECONDS: u64 = 120;
const IMAGE_BUILD_TIMEOUT_SECONDS: u64 = 1_800;
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

impl ContainerEventSource for BollardEngineAdapter {
    fn stream_managed<'stream>(
        &'stream self,
        installation_id: &'stream str,
        cursor: ContainerEventCursor,
    ) -> ContainerEventStream<'stream> {
        let request = match managed_container_events_request(installation_id, &cursor) {
            Ok(request) => request,
            Err(error) => {
                return Box::pin(futures_util::stream::once(async move { Err(error) }));
            }
        };

        Box::pin(self.docker.events(Some(request)).filter_map(move |result| {
            let event = match result {
                Ok(message) => container_event(message, &cursor),
                Err(error) => Err(backend_error("stream managed container events", error)),
            };

            async move {
                match event {
                    Ok(Some(event)) => Some(Ok(event)),
                    Ok(None) => None,
                    Err(error) => Some(Err(error)),
                }
            }
        }))
    }
}

impl LogSource for BollardEngineAdapter {
    fn logs<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        options: &'operation ContainerLogOptions,
    ) -> EngineFuture<'operation, ContainerLogStream<'operation>> {
        Box::pin(async move {
            bounded_engine_operation("open container logs", request_timeout(), async {
                let observed = self
                    .docker
                    .inspect_container(container.id().as_str(), None)
                    .await
                    .map_err(|error| backend_error("verify container log ownership", error))?;
                verify_owned_container_labels_for(
                    "read logs from",
                    container,
                    &observed
                        .config
                        .and_then(|config| config.labels)
                        .unwrap_or_default(),
                )
            })
            .await?;

            let stream = self
                .docker
                .logs(container.id().as_str(), Some(log_request(options)))
                .map(|result| {
                    result
                        .map(log_chunk)
                        .map_err(|error| backend_error("stream container logs", error))
                });

            let stream: ContainerLogStream<'operation> = Box::pin(stream);

            Ok(stream)
        })
    }
}

impl CommandExecutor for BollardEngineAdapter {
    fn start_command<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
        request: &'operation CommandRequest,
    ) -> EngineFuture<'operation, CommandSession> {
        Box::pin(async move {
            let (execution_id, started) =
                bounded_engine_operation("start container command", request_timeout(), async {
                    let observed = self
                        .docker
                        .inspect_container(container.id().as_str(), None)
                        .await
                        .map_err(|error| {
                            backend_error("verify container command ownership", error)
                        })?;
                    verify_owned_container_labels_for(
                        "execute a command in",
                        container,
                        &observed
                            .config
                            .and_then(|config| config.labels)
                            .unwrap_or_default(),
                    )?;

                    let created = self
                        .docker
                        .create_exec(container.id().as_str(), command_create_request(request))
                        .await
                        .map_err(|error| backend_error("create container command", error))?;

                    if created.id.is_empty() {
                        return Err(EngineError::Backend {
                            detail: "Engine created a container command without an ID".to_owned(),
                        });
                    }

                    let started = self
                        .docker
                        .start_exec(
                            &created.id,
                            Some(StartExecOptions {
                                detach: false,
                                tty: false,
                                output_capacity: None,
                            }),
                        )
                        .await
                        .map_err(|error| backend_error("start container command", error))?;

                    Ok((CommandExecutionId::new(created.id), started))
                })
                .await?;

            match started {
                StartExecResults::Attached { output, input } => {
                    let output = output.map(|result| {
                        result
                            .map(log_chunk)
                            .map_err(|error| backend_error("stream container command", error))
                    });
                    let output: ContainerLogStream<'static> = Box::pin(output);

                    Ok(CommandSession::new(
                        execution_id,
                        container.id().clone(),
                        input,
                        output,
                    ))
                }
                StartExecResults::Detached => Err(EngineError::Backend {
                    detail: "Engine detached an attached container command".to_owned(),
                }),
            }
        })
    }

    fn command_status<'operation>(
        &'operation self,
        execution_id: &'operation CommandExecutionId,
        container_id: &'operation ContainerId,
    ) -> EngineFuture<'operation, CommandStatus> {
        Box::pin(async move {
            bounded_engine_operation("inspect container command", request_timeout(), async {
                let observed = self
                    .docker
                    .inspect_exec(execution_id.as_str())
                    .await
                    .map_err(|error| backend_error("inspect container command", error))?;

                if observed.container_id.as_deref() != Some(container_id.as_str()) {
                    return Err(EngineError::Backend {
                        detail: format!(
                            "Engine command '{}' no longer belongs to container '{}'",
                            execution_id.as_str(),
                            container_id.as_str()
                        ),
                    });
                }

                match (observed.running, observed.exit_code) {
                    (Some(true), _) => Ok(CommandStatus::Running),
                    (Some(false), Some(exit_code)) => Ok(CommandStatus::Exited(exit_code)),
                    _ => Err(EngineError::Backend {
                        detail: format!(
                            "Engine returned incomplete status for container command '{}'",
                            execution_id.as_str()
                        ),
                    }),
                }
            })
            .await
        })
    }
}

impl HealthObserver for BollardEngineAdapter {
    fn observe_health<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ContainerHealth> {
        Box::pin(async move {
            bounded_engine_operation("observe container health", request_timeout(), async {
                let observed = match self
                    .docker
                    .inspect_container(container.id().as_str(), None)
                    .await
                {
                    Ok(observed) => observed,
                    Err(BollardError::DockerResponseServerError {
                        status_code: 404, ..
                    }) => return Ok(ContainerHealth::Missing),
                    Err(error) => return Err(backend_error("inspect container health", error)),
                };
                verify_owned_container_labels_for(
                    "observe health for",
                    container,
                    &observed
                        .config
                        .and_then(|config| config.labels)
                        .unwrap_or_default(),
                )?;

                container_health(observed.state.as_ref())
            })
            .await
        })
    }
}

impl ResourceMetrics for BollardEngineAdapter {
    fn sample_resources<'operation>(
        &'operation self,
        container: &'operation OwnedContainer,
    ) -> EngineFuture<'operation, ContainerResourceMetrics> {
        Box::pin(async move {
            bounded_engine_operation("sample container resources", request_timeout(), async {
                let observed = self
                    .docker
                    .inspect_container(container.id().as_str(), None)
                    .await
                    .map_err(|error| backend_error("verify container metric ownership", error))?;
                verify_owned_container_labels_for(
                    "measure resources for",
                    container,
                    &observed
                        .config
                        .and_then(|config| config.labels)
                        .unwrap_or_default(),
                )?;

                let options = StatsOptionsBuilder::default()
                    .stream(false)
                    .one_shot(false)
                    .build();
                let mut samples = self.docker.stats(container.id().as_str(), Some(options));
                let sample = samples
                    .next()
                    .await
                    .ok_or_else(|| EngineError::Backend {
                        detail: "Engine returned no container resource sample".to_owned(),
                    })?
                    .map_err(|error| backend_error("sample container resources", error))?;

                container_resource_metrics(&sample, container.id())
            })
            .await
        })
    }
}

pub(super) fn container_resource_metrics(
    stats: &ContainerStatsResponse,
    expected_container_id: &ContainerId,
) -> Result<ContainerResourceMetrics, EngineError> {
    let observed_container_id = stats.id.as_deref().ok_or_else(|| EngineError::Backend {
        detail: "Engine returned container stats without an ID".to_owned(),
    })?;

    if observed_container_id != expected_container_id.as_str() {
        return Err(EngineError::Backend {
            detail: format!(
                "Engine stats belong to container '{observed_container_id}', expected '{}'",
                expected_container_id.as_str()
            ),
        });
    }

    let memory_usage_bytes = stats.memory_stats.as_ref().and_then(|memory| {
        memory
            .usage
            .or(memory.privateworkingset)
            .or(memory.commitbytes)
    });
    let process_count = stats.pids_stats.as_ref().and_then(|pids| pids.current);
    let network_received_bytes = sum_network_bytes(stats, |network| network.rx_bytes)?;
    let network_transmitted_bytes = sum_network_bytes(stats, |network| network.tx_bytes)?;

    Ok(ContainerResourceMetrics::new(
        cpu_usage_basis_points(stats)?,
        memory_usage_bytes,
        process_count,
        network_received_bytes,
        network_transmitted_bytes,
    ))
}

fn cpu_usage_basis_points(stats: &ContainerStatsResponse) -> Result<Option<u64>, EngineError> {
    let Some(current) = stats.cpu_stats.as_ref() else {
        return Ok(None);
    };
    let Some(previous) = stats.precpu_stats.as_ref() else {
        return Ok(None);
    };
    let Some(cpu_delta) = cpu_total_usage(current).and_then(|current| {
        cpu_total_usage(previous).and_then(|previous| current.checked_sub(previous))
    }) else {
        return Ok(None);
    };
    let Some(system_delta) = current
        .system_cpu_usage
        .and_then(|current| {
            previous
                .system_cpu_usage
                .and_then(|previous| current.checked_sub(previous))
        })
        .filter(|delta| *delta > 0)
    else {
        return Ok(None);
    };
    let processors = u64::from(current.online_cpus.unwrap_or(1));
    let basis_points = u128::from(cpu_delta)
        .checked_mul(u128::from(processors))
        .and_then(|value| value.checked_mul(10_000))
        .map(|value| value / u128::from(system_delta))
        .and_then(|value| u64::try_from(value).ok())
        .ok_or_else(|| EngineError::Backend {
            detail: "Engine container CPU metrics overflowed normalization".to_owned(),
        })?;

    Ok(Some(basis_points))
}

fn cpu_total_usage(stats: &ContainerCpuStats) -> Option<u64> {
    stats.cpu_usage.as_ref().and_then(|usage| usage.total_usage)
}

fn sum_network_bytes(
    stats: &ContainerStatsResponse,
    select: fn(&ContainerNetworkStats) -> Option<u64>,
) -> Result<Option<u64>, EngineError> {
    let Some(networks) = stats.networks.as_ref() else {
        return Ok(None);
    };
    let mut total = 0_u64;

    for network in networks.values() {
        let Some(bytes) = select(network) else {
            return Ok(None);
        };
        total = total
            .checked_add(bytes)
            .ok_or_else(|| EngineError::Backend {
                detail: "Engine container network metrics overflowed aggregation".to_owned(),
            })?;
    }

    Ok(Some(total))
}

pub(super) fn container_health(
    state: Option<&EngineContainerState>,
) -> Result<ContainerHealth, EngineError> {
    let state = state.ok_or_else(|| EngineError::Backend {
        detail: "Engine returned container health without process state".to_owned(),
    })?;

    match state.running {
        Some(false) => return Ok(ContainerHealth::Stopped),
        Some(true) => {}
        None => {
            return Err(EngineError::Backend {
                detail: "Engine returned container health without running state".to_owned(),
            });
        }
    }

    match state.health.as_ref().and_then(|health| health.status) {
        None | Some(HealthStatusEnum::EMPTY | HealthStatusEnum::NONE) => {
            Ok(ContainerHealth::RunningUnverified)
        }
        Some(HealthStatusEnum::STARTING) => Ok(ContainerHealth::Starting),
        Some(HealthStatusEnum::HEALTHY) => Ok(ContainerHealth::Healthy),
        Some(HealthStatusEnum::UNHEALTHY) => {
            let failing_streak = state
                .health
                .as_ref()
                .and_then(|health| health.failing_streak)
                .unwrap_or_default();
            let failing_streak =
                u64::try_from(failing_streak).map_err(|_| EngineError::Backend {
                    detail: "Engine returned a negative container health failing streak".to_owned(),
                })?;

            Ok(ContainerHealth::Unhealthy { failing_streak })
        }
    }
}

pub(super) fn command_create_request(request: &CommandRequest) -> CreateExecOptions<String> {
    CreateExecOptions {
        attach_stdin: Some(true),
        attach_stdout: Some(true),
        attach_stderr: Some(true),
        tty: Some(false),
        env: Some(
            request
                .environment()
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect(),
        ),
        cmd: Some(request.arguments().to_vec()),
        privileged: Some(false),
        working_dir: request.working_directory().map(str::to_owned),
        ..CreateExecOptions::default()
    }
}

pub(super) fn log_request(options: &ContainerLogOptions) -> LogsOptions {
    LogsOptionsBuilder::default()
        .follow(options.follow())
        .stdout(true)
        .stderr(true)
        .tail(&options.tail().engine_value())
        .build()
}

pub(super) fn log_chunk(output: LogOutput) -> LogChunk {
    match output {
        LogOutput::StdOut { message } => LogChunk::new(LogStreamKind::Stdout, message.to_vec()),
        LogOutput::StdErr { message } => LogChunk::new(LogStreamKind::Stderr, message.to_vec()),
        LogOutput::StdIn { message } => LogChunk::new(LogStreamKind::Stdin, message.to_vec()),
        LogOutput::Console { message } => LogChunk::new(LogStreamKind::Console, message.to_vec()),
    }
}

pub(super) fn managed_container_events_request(
    installation_id: &str,
    cursor: &ContainerEventCursor,
) -> Result<EventsOptions, EngineError> {
    if installation_id.is_empty() {
        return Err(EngineError::InvalidRequest {
            detail: "managed event installation ID must not be empty".to_owned(),
        });
    }

    let filters = HashMap::from([
        ("type", vec!["container".to_owned()]),
        (
            "label",
            vec![
                format!("{MANAGED_LABEL}=true"),
                format!("{INSTALLATION_LABEL}={installation_id}"),
            ],
        ),
    ]);
    let mut builder = EventsOptionsBuilder::default().filters(&filters);

    if let Some(since) = cursor.since_seconds() {
        builder = builder.since(&since);
    }

    Ok(builder.build())
}

pub(super) fn container_event(
    message: EventMessage,
    cursor: &ContainerEventCursor,
) -> Result<Option<ContainerEvent>, EngineError> {
    if message.typ != Some(EventMessageTypeEnum::CONTAINER) {
        return Ok(None);
    }

    let actor = message
        .actor
        .ok_or_else(|| malformed_event("missing actor"))?;
    let container_id = actor
        .id
        .filter(|id| !id.is_empty())
        .ok_or_else(|| malformed_event("missing container ID"))?;
    let action = message
        .action
        .filter(|action| !action.is_empty())
        .map(ContainerEventAction::from_engine_action)
        .ok_or_else(|| malformed_event("missing action"))?;
    let occurred_at_nanoseconds = message
        .time_nano
        .or_else(|| {
            message
                .time
                .and_then(|seconds| seconds.checked_mul(1_000_000_000))
        })
        .and_then(|timestamp| u64::try_from(timestamp).ok())
        .ok_or_else(|| malformed_event("missing or invalid timestamp"))?;

    if cursor.has_processed(&container_id, &action, occurred_at_nanoseconds) {
        return Ok(None);
    }

    Ok(Some(ContainerEvent::new(
        ContainerId::new(container_id),
        action,
        occurred_at_nanoseconds,
    )))
}

fn malformed_event(detail: &str) -> EngineError {
    EngineError::Backend {
        detail: format!("Engine returned malformed managed container event: {detail}"),
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

impl ImageBuilder for BollardEngineAdapter {
    fn build_image<'operation>(
        &'operation self,
        request: &'operation ImageBuildRequest,
    ) -> EngineFuture<'operation, ImageId> {
        Box::pin(async move {
            bounded_engine_operation("build derived image", image_build_timeout(), async {
                match self.docker.inspect_image(request.output_tag()).await {
                    Ok(image) => return verified_built_image(image, request, "reuse"),
                    Err(BollardError::DockerResponseServerError {
                        status_code: 404, ..
                    }) => {}
                    Err(error) => {
                        return Err(backend_error("inspect derived image cache", error));
                    }
                }

                let events = self
                    .docker
                    .build_image(
                        build_image_options(request),
                        None,
                        Some(body_full(request.context_tar().to_vec().into())),
                    )
                    .try_collect::<Vec<_>>()
                    .await
                    .map_err(|error| backend_error("build derived image", error))?;

                if let Some(detail) = events.into_iter().find_map(|event| event.error_detail) {
                    return Err(EngineError::Backend {
                        detail: detail
                            .message
                            .unwrap_or_else(|| "Engine derived image build failed".to_owned()),
                    });
                }

                let image = self
                    .docker
                    .inspect_image(request.output_tag())
                    .await
                    .map_err(|error| backend_error("inspect built derived image", error))?;

                verified_built_image(image, request, "use")
            })
            .await
        })
    }
}

pub(super) fn build_image_options(request: &ImageBuildRequest) -> BuildImageOptions {
    let labels = request.labels().into_iter().collect::<HashMap<_, _>>();

    BuildImageOptionsBuilder::default()
        .dockerfile(request.dockerfile_path())
        .t(request.output_tag())
        .pull("false")
        .rm(true)
        .forcerm(true)
        .labels(&labels)
        .networkmode("none")
        .platform(request.platform())
        .build()
}

fn verified_built_image(
    image: bollard::models::ImageInspect,
    request: &ImageBuildRequest,
    action: &'static str,
) -> Result<ImageId, EngineError> {
    let labels = image
        .config
        .and_then(|config| config.labels)
        .unwrap_or_default();

    if !request
        .labels()
        .iter()
        .all(|(key, value)| labels.get(key) == Some(value))
    {
        return Err(EngineError::OwnershipMismatch {
            action,
            resource_kind: "image",
            resource_id: request.output_tag().to_owned(),
        });
    }

    image
        .id
        .map(ImageId::new)
        .ok_or_else(|| EngineError::Backend {
            detail: format!(
                "Engine returned derived image '{}' without an ID",
                request.output_tag()
            ),
        })
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
    verify_owned_container_labels_for("mutate", container, labels)
}

fn verify_owned_container_labels_for(
    action: &'static str,
    container: &OwnedContainer,
    labels: &HashMap<String, String>,
) -> Result<(), EngineError> {
    if labels_match_metadata(labels, container.metadata()) {
        return Ok(());
    }

    Err(EngineError::OwnershipMismatch {
        action,
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
    let mut query = CreateContainerOptionsBuilder::default().name(options.name());
    if let Some(platform) = options.platform() {
        query = query.platform(platform);
    }
    let query = query.build();
    let body = ContainerCreateBody {
        image: Some(options.image().to_owned()),
        cmd: (!options.command().is_empty()).then(|| options.command().to_vec()),
        env: (!options.environment().is_empty()).then(|| {
            options
                .environment()
                .iter()
                .map(|(key, value)| format!("{key}={value}"))
                .collect()
        }),
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

    let mounts = options
        .bind_mounts()
        .iter()
        .map(|mount| Mount {
            source: Some(mount.source().to_owned()),
            target: Some(mount.target().to_owned()),
            typ: Some(MountType::BIND),
            read_only: Some(mount.is_read_only()),
            ..Mount::default()
        })
        .chain(options.volume_mounts().iter().map(|mount| Mount {
            source: Some(mount.source().to_owned()),
            target: Some(mount.target().to_owned()),
            typ: Some(MountType::VOLUME),
            read_only: Some(mount.is_read_only()),
            ..Mount::default()
        }))
        .collect::<Vec<_>>();

    HostConfig {
        network_mode: options.network().map(str::to_owned),
        port_bindings: (!port_bindings.is_empty()).then_some(port_bindings),
        mounts: (!mounts.is_empty()).then_some(mounts),
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

const fn image_build_timeout() -> Duration {
    Duration::from_secs(IMAGE_BUILD_TIMEOUT_SECONDS)
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
