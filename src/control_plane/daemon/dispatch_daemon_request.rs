use super::record_ipc_event::record_ipc_event;
use super::{DaemonRequestDispatchOptions, QueuedProjectCommand, reconcile_watched_roots};
use crate::control_plane::application::ControlPlane;
use crate::control_plane::daemon::ipc::{
    IpcDiagnostic, IpcEventKind, IpcPayload, IpcProjectCommand, IpcProjectStatus,
    IpcResourceLifecycle, IpcResourceStatus, IpcResponse, IpcResult,
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
        IpcPayload::ProjectStatus { canonical_path } => {
            match project_status(control_plane, canonical_path) {
                Ok(project) => {
                    IpcResponse::success(request.request_id(), IpcResult::ProjectStatus { project })
                }
                Err(message) => IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new("project_status_failed", message, false)],
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
        IpcPayload::Cancel { .. } => IpcResponse::failure(
            request.request_id(),
            vec![IpcDiagnostic::new(
                "operation_not_available",
                "the singleton daemon does not support this operation yet",
                false,
            )],
        ),
    }
}

fn project_status<Store>(
    control_plane: &ControlPlane<Store>,
    canonical_path: &std::path::Path,
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
            IpcResourceStatus::new(
                resource
                    .scope_id()
                    .unwrap_or_else(|| resource.resource_id())
                    .to_owned(),
                resource.kind().to_owned(),
                ipc_resource_lifecycle(resource.lifecycle()),
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
                IpcResourceStatus::new(
                    resource.service_id().to_owned(),
                    resource.kind().to_owned(),
                    ipc_resource_lifecycle(resource.lifecycle()),
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
        IpcProjectCommand::Hook { name, arguments } => ProjectCommand::Hook {
            name: name.clone(),
            arguments: arguments.clone(),
        },
    }
}
