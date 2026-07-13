use super::record_ipc_event::record_ipc_event;
use super::{
    DaemonRequestDispatchOptions, ProjectLogRequest, ProjectLogSessionRegistryError,
    ProjectLogTarget, QueuedProjectCommand, ResourceHealthRegistry, reconcile_watched_roots,
};
use crate::control_plane::application::ControlPlane;
use crate::control_plane::daemon::ipc::{
    IpcDiagnostic, IpcEventKind, IpcManagedEnvironment, IpcPayload, IpcProjectCommand,
    IpcProjectStatus, IpcResourceHealth, IpcResourceLifecycle, IpcResourceStatus, IpcResponse,
    IpcResult,
};
use crate::control_plane::state::{
    DaemonOperationRecord, DaemonOperationRecordOptions, DaemonOperationStatus,
    DaemonOperationTransitionOptions, EnvironmentLifecycle, ResourceLifecycle, StateStore,
};
use crate::control_plane::workload::{
    ProjectCommand, ProjectCommandPlan, ProjectCommandPlanOptions,
};
use crate::control_plane::{ProjectIdentity, ServiceIdentity};
use std::time::Duration;

const MAX_PROJECT_COMMAND_TIMEOUT_SECONDS: u64 = 3_600;
const MAX_RESOURCE_HEALTH_AGE_SECONDS: i64 = 120;

/// Dispatches one correlated request without allowing partial registry mutation.
pub(crate) fn dispatch_daemon_request<Store>(
    options: DaemonRequestDispatchOptions<'_, Store>,
) -> IpcResponse
where
    Store: StateStore,
{
    let DaemonRequestDispatchOptions {
        control_plane,
        discovery_options,
        request,
        event_journal,
        project_commands,
        project_logs,
        resource_health,
        image_reference_resolution,
        now_unix_seconds,
    } = options;
    match request.payload() {
        IpcPayload::Ping => IpcResponse::success(request.request_id(), IpcResult::Pong),
        IpcPayload::Reconcile => {
            if let Err(error) = record_ipc_event(
                control_plane,
                event_journal,
                request.request_id(),
                IpcEventKind::Accepted,
            ) {
                return IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "event_journal_failed",
                        error.to_string(),
                        false,
                    )],
                );
            }
            match reconcile_watched_roots(control_plane, discovery_options, now_unix_seconds) {
                Ok(result) => {
                    if let Err(error) = record_ipc_event(
                        control_plane,
                        event_journal,
                        request.request_id(),
                        IpcEventKind::Completed,
                    ) {
                        return IpcResponse::failure(
                            request.request_id(),
                            vec![IpcDiagnostic::new(
                                "event_journal_failed",
                                error.to_string(),
                                false,
                            )],
                        );
                    }
                    IpcResponse::success(
                        request.request_id(),
                        IpcResult::Reconciled {
                            project_count: result.report().sources().len(),
                            issue_count: result.report().issues().len(),
                            applied: result.was_applied(),
                        },
                    )
                }
                Err(error) => {
                    let message = error.to_string();
                    if let Err(journal_error) = record_ipc_event(
                        control_plane,
                        event_journal,
                        request.request_id(),
                        IpcEventKind::Failed {
                            code: "reconciliation_failed".to_owned(),
                            message: message.clone(),
                        },
                    ) {
                        return IpcResponse::failure(
                            request.request_id(),
                            vec![IpcDiagnostic::new(
                                "event_journal_failed",
                                journal_error.to_string(),
                                false,
                            )],
                        );
                    }
                    IpcResponse::failure(
                        request.request_id(),
                        vec![IpcDiagnostic::new("reconciliation_failed", message, true)],
                    )
                }
            }
        }
        IpcPayload::ResolveImageReferences { references } => {
            let Some(resolver) = image_reference_resolution else {
                return IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "engine_unavailable",
                        "the selected container Engine is unavailable; retry after it reconnects",
                        true,
                    )],
                );
            };
            if references.is_empty() {
                return IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "image_resolution_failed",
                        "at least one mutable image reference is required",
                        false,
                    )],
                );
            }

            match resolver.resolve(references) {
                Ok(references) => IpcResponse::success(
                    request.request_id(),
                    IpcResult::ImageReferencesResolved { references },
                ),
                Err(message) => IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "image_resolution_failed",
                        message,
                        false,
                    )],
                ),
            }
        }
        IpcPayload::ProjectStatus { canonical_path } => {
            match project_status(
                control_plane,
                resource_health,
                canonical_path,
                now_unix_seconds,
            ) {
                Ok(project) => {
                    IpcResponse::success(request.request_id(), IpcResult::ProjectStatus { project })
                }
                Err(message) => IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new("project_status_failed", message, false)],
                ),
            }
        }
        IpcPayload::ProjectEnvironment { canonical_path } => {
            match project_environment(control_plane, canonical_path) {
                Ok(environment) => IpcResponse::success(
                    request.request_id(),
                    IpcResult::ProjectEnvironment { environment },
                ),
                Err(message) => IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "project_environment_failed",
                        message,
                        false,
                    )],
                ),
            }
        }
        IpcPayload::AdoptProject { canonical_path } => {
            if let Err(error) = record_ipc_event(
                control_plane,
                event_journal,
                request.request_id(),
                IpcEventKind::Accepted,
            ) {
                return IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "event_journal_failed",
                        error.to_string(),
                        false,
                    )],
                );
            }
            match control_plane.adopt_project(canonical_path) {
                Ok(project_id) => {
                    if let Err(error) = record_ipc_event(
                        control_plane,
                        event_journal,
                        request.request_id(),
                        IpcEventKind::Completed,
                    ) {
                        return IpcResponse::failure(
                            request.request_id(),
                            vec![IpcDiagnostic::new(
                                "event_journal_failed",
                                error.to_string(),
                                false,
                            )],
                        );
                    }
                    IpcResponse::success(
                        request.request_id(),
                        IpcResult::ProjectAdopted { project_id },
                    )
                }
                Err(error) => {
                    let message = error.to_string();
                    if let Err(journal_error) = record_ipc_event(
                        control_plane,
                        event_journal,
                        request.request_id(),
                        IpcEventKind::Failed {
                            code: "project_adoption_failed".to_owned(),
                            message: message.clone(),
                        },
                    ) {
                        return IpcResponse::failure(
                            request.request_id(),
                            vec![IpcDiagnostic::new(
                                "event_journal_failed",
                                journal_error.to_string(),
                                false,
                            )],
                        );
                    }
                    IpcResponse::failure(
                        request.request_id(),
                        vec![IpcDiagnostic::new(
                            "project_adoption_failed",
                            message,
                            false,
                        )],
                    )
                }
            }
        }
        IpcPayload::SubscribeEvents { after_sequence } => {
            match event_journal.events_after(*after_sequence) {
                Ok(events) => IpcResponse::success(
                    request.request_id(),
                    IpcResult::Events {
                        events,
                        latest_sequence: event_journal.latest_sequence(),
                    },
                ),
                Err(error) => IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "event_cursor_unavailable",
                        error.to_string(),
                        false,
                    )],
                ),
            }
        }
        IpcPayload::RunProjectCommand {
            canonical_path,
            service,
            command,
            timeout_seconds,
        } => {
            let queued = match prepare_project_command(
                control_plane,
                request.request_id(),
                canonical_path,
                service,
                command,
                *timeout_seconds,
            ) {
                Ok(queued) => queued,
                Err(message) => {
                    return IpcResponse::failure(
                        request.request_id(),
                        vec![IpcDiagnostic::new(
                            "project_command_invalid",
                            message,
                            false,
                        )],
                    );
                }
            };
            let payload_json = match queued.payload_json() {
                Ok(payload) => payload,
                Err(message) => {
                    return IpcResponse::failure(
                        request.request_id(),
                        vec![IpcDiagnostic::new(
                            "project_command_invalid",
                            message,
                            false,
                        )],
                    );
                }
            };
            if let Err(error) = project_commands.enqueue(queued) {
                return IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "project_command_queue_unavailable",
                        error.to_string(),
                        true,
                    )],
                );
            }
            let accepted_kind_json = serde_json::to_string(&IpcEventKind::Accepted)
                .expect("accepted event serialization is infallible");
            let operation = DaemonOperationRecord::new(DaemonOperationRecordOptions {
                operation_id: request.request_id().to_owned(),
                kind: "project_command".to_owned(),
                payload_json,
                status: DaemonOperationStatus::Queued,
                created_at_unix_seconds: now_unix_seconds,
                updated_at_unix_seconds: now_unix_seconds,
            });
            let accepted = match control_plane.enqueue_daemon_operation(
                &operation,
                &accepted_kind_json,
                event_journal.capacity(),
            ) {
                Ok(event) => event,
                Err(error) => {
                    drop(project_commands.remove(request.request_id()));

                    return IpcResponse::failure(
                        request.request_id(),
                        vec![IpcDiagnostic::new(
                            "project_command_queue_unavailable",
                            error.to_string(),
                            true,
                        )],
                    );
                }
            };
            if let Err(error) = event_journal.append_record(accepted) {
                drop(project_commands.remove(request.request_id()));
                let failed = IpcEventKind::Failed {
                    code: "event_journal_failed".to_owned(),
                    message: error.to_string(),
                };
                let failed_json = serde_json::to_string(&failed)
                    .expect("event journal failure serialization is infallible");
                if let Ok(Some(event)) =
                    control_plane.transition_daemon_operation(DaemonOperationTransitionOptions {
                        operation_id: request.request_id(),
                        expected: DaemonOperationStatus::Queued,
                        next: DaemonOperationStatus::Failed,
                        updated_at_unix_seconds: now_unix_seconds,
                        event_kind_json: Some(&failed_json),
                        event_retention_limit: event_journal.capacity(),
                    })
                {
                    drop(event_journal.append_record(event));
                }

                return IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "event_journal_failed",
                        error.to_string(),
                        false,
                    )],
                );
            }

            IpcResponse::success(
                request.request_id(),
                IpcResult::Accepted {
                    operation_id: request.request_id().to_owned(),
                },
            )
        }
        IpcPayload::OpenProjectLogs {
            canonical_path,
            services,
            follow,
            tail,
        } => {
            let log_request = match prepare_project_logs(
                control_plane,
                request.request_id(),
                canonical_path,
                services,
                *follow,
                *tail,
            ) {
                Ok(log_request) => log_request,
                Err(message) => {
                    return IpcResponse::failure(
                        request.request_id(),
                        vec![IpcDiagnostic::new("project_logs_invalid", message, false)],
                    );
                }
            };
            match project_logs.open(log_request) {
                Ok(()) => IpcResponse::success(
                    request.request_id(),
                    IpcResult::Accepted {
                        operation_id: request.request_id().to_owned(),
                    },
                ),
                Err(error) => IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "project_logs_unavailable",
                        error.to_string(),
                        true,
                    )],
                ),
            }
        }
        IpcPayload::PollProjectLogs {
            session_id,
            after_sequence,
            max_chunks,
        } => match project_logs.poll(session_id, *after_sequence, usize::from(*max_chunks)) {
            Ok((chunks, latest_sequence, state)) => IpcResponse::success(
                request.request_id(),
                IpcResult::ProjectLogs {
                    session_id: session_id.clone(),
                    chunks,
                    latest_sequence,
                    state,
                },
            ),
            Err(error) => IpcResponse::failure(
                request.request_id(),
                vec![IpcDiagnostic::new(
                    "project_logs_poll_failed",
                    error.to_string(),
                    false,
                )],
            ),
        },
        IpcPayload::Cancel { target_request_id } => match project_logs.cancel(target_request_id) {
            Ok(()) => IpcResponse::success(
                request.request_id(),
                IpcResult::Accepted {
                    operation_id: target_request_id.clone(),
                },
            ),
            Err(ProjectLogSessionRegistryError::UnknownSession { .. }) => IpcResponse::failure(
                request.request_id(),
                vec![IpcDiagnostic::new(
                    "operation_not_available",
                    "the singleton daemon does not support this operation yet",
                    false,
                )],
            ),
            Err(error) => IpcResponse::failure(
                request.request_id(),
                vec![IpcDiagnostic::new(
                    "project_logs_cancel_failed",
                    error.to_string(),
                    false,
                )],
            ),
        },
    }
}

fn prepare_project_logs<Store>(
    control_plane: &ControlPlane<Store>,
    session_id: &str,
    canonical_path: &std::path::Path,
    services: &[String],
    follow: bool,
    tail: Option<u32>,
) -> Result<ProjectLogRequest, String>
where
    Store: StateStore,
{
    if services.is_empty() {
        return Err("project logs require at least one exact service".to_owned());
    }
    if tail == Some(0) {
        return Err("project log tail must be greater than zero".to_owned());
    }
    let project = control_plane
        .projects()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|project| project.canonical_path() == canonical_path)
        .ok_or_else(|| {
            format!(
                "project path '{}' is not registered by the singleton daemon",
                canonical_path.display()
            )
        })?;
    let resources = control_plane
        .resources()
        .map_err(|error| error.to_string())?;
    let logical_resources = control_plane
        .logical_resources()
        .map_err(|error| error.to_string())?;
    let mut seen = std::collections::BTreeSet::new();
    let mut targets = Vec::with_capacity(services.len());

    for requested in services {
        let service = ServiceIdentity::new(requested).map_err(|error| error.to_string())?;
        if !seen.insert(service.as_str().to_owned()) {
            return Err(format!(
                "project log service '{}' was requested more than once",
                service.as_str()
            ));
        }
        let mut matches = resources
            .iter()
            .filter(|resource| {
                resource.project_id() == Some(project.project_name())
                    && resource.scope_id() == Some(service.as_str())
                    && resource.lifecycle() == ResourceLifecycle::Active
                    && matches!(
                        resource.kind(),
                        "project_application" | "project_process" | "project_service"
                    )
            })
            .map(|resource| {
                ProjectLogTarget::new(
                    service.as_str().to_owned(),
                    resource.resource_id().to_owned(),
                    Some(project.project_name().to_owned()),
                )
            })
            .collect::<Vec<_>>();
        matches.extend(
            logical_resources
                .iter()
                .filter(|logical| {
                    logical.project_id() == project.project_name()
                        && logical.service_id() == service.as_str()
                        && logical.lifecycle() == ResourceLifecycle::Active
                })
                .filter_map(|logical| {
                    resources
                        .iter()
                        .find(|resource| {
                            resource.kind() == "shared_service"
                                && resource.project_id().is_none()
                                && resource.compatibility_fingerprint()
                                    == logical.compatibility_fingerprint()
                                && resource.lifecycle() == ResourceLifecycle::Active
                        })
                        .map(|resource| {
                            ProjectLogTarget::new(
                                service.as_str().to_owned(),
                                resource.resource_id().to_owned(),
                                None,
                            )
                        })
                }),
        );
        match matches.as_slice() {
            [target] => targets.push(target.clone()),
            [] => {
                return Err(format!(
                    "project '{}' service '{}' has no active owned container",
                    project.project_name(),
                    service.as_str()
                ));
            }
            _ => {
                return Err(format!(
                    "project '{}' service '{}' resolves to multiple owned containers",
                    project.project_name(),
                    service.as_str()
                ));
            }
        }
    }

    Ok(ProjectLogRequest::new(
        session_id.to_owned(),
        project.project_name().to_owned(),
        targets,
        follow,
        tail,
    ))
}

fn project_environment<Store>(
    control_plane: &ControlPlane<Store>,
    canonical_path: &std::path::Path,
) -> Result<IpcManagedEnvironment, String>
where
    Store: StateStore,
{
    let project = control_plane
        .projects()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|project| project.canonical_path() == canonical_path)
        .ok_or_else(|| {
            format!(
                "project path '{}' is not registered by the singleton daemon",
                canonical_path.display()
            )
        })?;
    let environment = control_plane
        .managed_environments()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|environment| {
            environment.project_id() == project.project_name()
                && environment.lifecycle() == EnvironmentLifecycle::Active
        })
        .ok_or_else(|| {
            format!(
                "project '{}' has no active managed environment; wait for reconciliation",
                project.project_name()
            )
        })?;

    Ok(IpcManagedEnvironment::new(
        environment.project_id().to_owned(),
        environment.revision().to_owned(),
        environment.values().clone(),
    ))
}

fn project_status<Store>(
    control_plane: &ControlPlane<Store>,
    resource_health: &ResourceHealthRegistry,
    canonical_path: &std::path::Path,
    now_unix_seconds: i64,
) -> Result<IpcProjectStatus, String>
where
    Store: StateStore,
{
    let project = control_plane
        .projects()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|project| project.canonical_path() == canonical_path)
        .ok_or_else(|| {
            format!(
                "project path '{}' is not registered by the singleton daemon",
                canonical_path.display()
            )
        })?;
    let mut resources = control_plane
        .resources()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|resource| resource.project_id() == Some(project.project_name()))
        .map(|resource| {
            let (health, observed_at) = ipc_resource_health(
                resource_health.observation(resource.resource_id()),
                now_unix_seconds,
            );
            IpcResourceStatus::new(
                resource
                    .scope_id()
                    .unwrap_or_else(|| resource.resource_id())
                    .to_owned(),
                resource.kind().to_owned(),
                ipc_resource_lifecycle(resource.lifecycle()),
                health,
                observed_at,
                false,
            )
        })
        .collect::<Vec<_>>();
    resources.extend(
        control_plane
            .logical_resources()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|resource| resource.project_id() == project.project_name())
            .map(|resource| {
                let (health, observed_at) = ipc_resource_health(
                    resource_health.observation(resource.shared_resource_id()),
                    now_unix_seconds,
                );
                IpcResourceStatus::new(
                    resource.service_id().to_owned(),
                    resource.kind().to_owned(),
                    ipc_resource_lifecycle(resource.lifecycle()),
                    health,
                    observed_at,
                    true,
                )
            }),
    );
    resources.sort_by(|left, right| {
        left.service()
            .cmp(right.service())
            .then_with(|| left.kind().cmp(right.kind()))
            .then_with(|| left.shared().cmp(&right.shared()))
    });

    Ok(IpcProjectStatus::new(
        project.project_name().to_owned(),
        project.route_domains().to_vec(),
        resources,
    ))
}

const fn ipc_resource_health(
    observation: Option<(crate::control_plane::engine::ContainerHealth, i64)>,
    now_unix_seconds: i64,
) -> (IpcResourceHealth, Option<i64>) {
    use crate::control_plane::engine::ContainerHealth;

    let Some((health, observed_at)) = observation else {
        return (IpcResourceHealth::Unknown, None);
    };
    if observed_at > now_unix_seconds
        || now_unix_seconds.saturating_sub(observed_at) > MAX_RESOURCE_HEALTH_AGE_SECONDS
    {
        return (IpcResourceHealth::Unknown, Some(observed_at));
    }
    let health = match health {
        ContainerHealth::Missing => IpcResourceHealth::Missing,
        ContainerHealth::Stopped => IpcResourceHealth::Stopped,
        ContainerHealth::RunningUnverified => IpcResourceHealth::RunningUnverified,
        ContainerHealth::Starting => IpcResourceHealth::Starting,
        ContainerHealth::Healthy => IpcResourceHealth::Healthy,
        ContainerHealth::Unhealthy { failing_streak } => {
            IpcResourceHealth::Unhealthy { failing_streak }
        }
    };

    (health, Some(observed_at))
}

const fn ipc_resource_lifecycle(lifecycle: ResourceLifecycle) -> IpcResourceLifecycle {
    match lifecycle {
        ResourceLifecycle::Active => IpcResourceLifecycle::Active,
        ResourceLifecycle::Orphaned => IpcResourceLifecycle::Orphaned,
        ResourceLifecycle::Retained => IpcResourceLifecycle::Retained,
    }
}

fn prepare_project_command<Store>(
    control_plane: &ControlPlane<Store>,
    operation_id: &str,
    canonical_path: &std::path::Path,
    service: &str,
    command: &IpcProjectCommand,
    timeout_seconds: u64,
) -> Result<QueuedProjectCommand, String>
where
    Store: StateStore,
{
    if timeout_seconds == 0 || timeout_seconds > MAX_PROJECT_COMMAND_TIMEOUT_SECONDS {
        return Err(format!(
            "project command timeout must be between 1 and {MAX_PROJECT_COMMAND_TIMEOUT_SECONDS} seconds"
        ));
    }
    let service = ServiceIdentity::new(service).map_err(|error| error.to_string())?;
    let projects = control_plane
        .projects()
        .map_err(|error| error.to_string())?;
    let project = projects
        .iter()
        .find(|project| project.canonical_path() == canonical_path)
        .ok_or_else(|| {
            format!(
                "project path '{}' is not registered by the singleton daemon",
                canonical_path.display()
            )
        })?;
    let environment = control_plane
        .managed_environments()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|environment| {
            environment.project_id() == project.project_name()
                && environment.lifecycle() == EnvironmentLifecycle::Active
        })
        .ok_or_else(|| {
            format!(
                "project '{}' has no active managed environment; wait for reconciliation",
                project.project_name()
            )
        })?;
    let identity = ProjectIdentity::resolve(Some(project.project_name()), project.canonical_path())
        .map_err(|error| error.to_string())?;
    let plan = ProjectCommandPlan::new(ProjectCommandPlanOptions {
        project: identity,
        command: workload_command(command),
        environment: environment.values().clone(),
        input: Vec::new(),
        timeout: Duration::from_secs(timeout_seconds),
        browser_session: matches!(command, IpcProjectCommand::Artisan { browser: true, .. }),
    })
    .map_err(|error| error.to_string())?;

    Ok(QueuedProjectCommand::new(
        operation_id.to_owned(),
        service.as_str().to_owned(),
        plan,
    ))
}

fn workload_command(command: &IpcProjectCommand) -> ProjectCommand {
    match command {
        IpcProjectCommand::Composer { arguments } => ProjectCommand::Composer {
            arguments: arguments.clone(),
        },
        IpcProjectCommand::NodePackageManager {
            package_manager,
            arguments,
        } => ProjectCommand::NodePackageManager {
            package_manager: match package_manager {
                super::ipc::IpcNodePackageManager::Npm => {
                    crate::control_plane::workload::NodePackageManager::Npm
                }
                super::ipc::IpcNodePackageManager::Pnpm => {
                    crate::control_plane::workload::NodePackageManager::Pnpm
                }
                super::ipc::IpcNodePackageManager::Yarn => {
                    crate::control_plane::workload::NodePackageManager::Yarn
                }
            },
            arguments: arguments.clone(),
        },
        IpcProjectCommand::Bun { arguments } => ProjectCommand::Bun {
            arguments: arguments.clone(),
        },
        IpcProjectCommand::Artisan { arguments, .. } => ProjectCommand::Artisan {
            arguments: arguments.clone(),
        },
        IpcProjectCommand::Exec { arguments } => ProjectCommand::Exec {
            arguments: arguments.clone(),
        },
        IpcProjectCommand::PhpTool { tool, arguments } => ProjectCommand::PhpTool {
            tool: match tool {
                super::ipc::IpcPhpTool::PhpStan => crate::control_plane::workload::PhpTool::PhpStan,
                super::ipc::IpcPhpTool::Ecs => crate::control_plane::workload::PhpTool::Ecs,
                super::ipc::IpcPhpTool::PhpCsFixer => {
                    crate::control_plane::workload::PhpTool::PhpCsFixer
                }
                super::ipc::IpcPhpTool::Psalm => crate::control_plane::workload::PhpTool::Psalm,
                super::ipc::IpcPhpTool::Pint => crate::control_plane::workload::PhpTool::Pint,
                super::ipc::IpcPhpTool::Pest => crate::control_plane::workload::PhpTool::Pest,
                super::ipc::IpcPhpTool::PhpUnit => crate::control_plane::workload::PhpTool::PhpUnit,
                super::ipc::IpcPhpTool::Rector => crate::control_plane::workload::PhpTool::Rector,
            },
            arguments: arguments.clone(),
        },
        IpcProjectCommand::Deno { arguments } => ProjectCommand::Deno {
            arguments: arguments.clone(),
        },
        IpcProjectCommand::Hook { name, arguments } => ProjectCommand::Hook {
            name: name.clone(),
            arguments: arguments.clone(),
        },
    }
}
