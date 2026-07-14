use super::record_ipc_event::record_ipc_event;
use super::{
    DaemonRequestDispatchOptions, ProjectLogRequest, ProjectLogSessionRegistryError,
    ProjectLogTarget, QueuedMigrationDecision, QueuedPostgresPrune, QueuedProjectBackup,
    QueuedProjectCommand, QueuedProjectRestore, QueuedProjectRestoreOptions,
    ResourceHealthRegistry, build_postgres_prune_plan, plan_postgres_prune,
    reconcile_watched_roots, retry_failed_installation_deletion_prune,
};
use crate::control_plane::application::ControlPlane;
use crate::control_plane::daemon::ipc::{
    IpcDataLifecycle, IpcDiagnostic, IpcEventKind, IpcInstallationDeletionPlan,
    IpcInstallationDeletionStatus, IpcInstallationLifecycle, IpcManagedEnvironment,
    IpcMigrationDecision, IpcMigrationStatus, IpcPayload, IpcProjectCommand, IpcProjectStatus,
    IpcRecoveryPoint, IpcResourceHealth, IpcResourceLifecycle, IpcResourceStatus, IpcResponse,
    IpcResult,
};
use crate::control_plane::state::{
    DaemonOperationRecord, DaemonOperationRecordOptions, DaemonOperationStatus,
    DaemonOperationTransitionOptions, EnvironmentLifecycle, MigrationPhase, ResourceLifecycle,
    ResourceRetention, StateStore,
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
        project_backups,
        postgres_prunes,
        project_restores,
        migration_decisions,
        project_logs,
        resource_health,
        benchmark_snapshot,
        image_reference_resolution,
        mut v7_project_inventory,
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
        IpcPayload::BenchmarkSnapshot => {
            let project_count = match control_plane.projects() {
                Ok(projects) => projects.len(),
                Err(error) => {
                    return IpcResponse::failure(
                        request.request_id(),
                        vec![IpcDiagnostic::new(
                            "benchmark_snapshot_failed",
                            error.to_string(),
                            true,
                        )],
                    );
                }
            };
            match benchmark_snapshot {
                Some(provider) => match provider.snapshot(project_count, now_unix_seconds) {
                    Ok(snapshot) => IpcResponse::success(
                        request.request_id(),
                        IpcResult::BenchmarkSnapshot { snapshot },
                    ),
                    Err(message) => IpcResponse::failure(
                        request.request_id(),
                        vec![IpcDiagnostic::new(
                            "benchmark_snapshot_failed",
                            message,
                            true,
                        )],
                    ),
                },
                None => IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "benchmark_engine_unavailable",
                        "the selected Engine is unavailable for a complete benchmark sample",
                        true,
                    )],
                ),
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
        IpcPayload::ProjectMigrations { canonical_path } => {
            match project_migrations(control_plane, canonical_path) {
                Ok(migrations) => IpcResponse::success(
                    request.request_id(),
                    IpcResult::ProjectMigrations { migrations },
                ),
                Err(message) => IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "project_migrations_failed",
                        message,
                        false,
                    )],
                ),
            }
        }
        IpcPayload::InventoryV7Project { canonical_path } => {
            if let Err(message) = validate_v7_inventory_path(control_plane, canonical_path) {
                return IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new("v7_inventory_failed", message, false)],
                );
            }
            let Some(provider) = v7_project_inventory.as_mut() else {
                return IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "v7_inventory_engine_unavailable",
                        "the selected Engine is unavailable for legacy inventory",
                        true,
                    )],
                );
            };
            match provider.inventory(canonical_path, discovery_options.maximum_config_bytes()) {
                Ok(inventory) => IpcResponse::success(
                    request.request_id(),
                    IpcResult::V7ProjectInventory { inventory },
                ),
                Err(message) => IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new("v7_inventory_failed", message, false)],
                ),
            }
        }
        IpcPayload::DecideProjectMigration {
            canonical_path,
            migration_id,
            decision,
        } => {
            let queued = match prepare_migration_decision(
                control_plane,
                request.request_id(),
                canonical_path,
                migration_id,
                *decision,
            ) {
                Ok(queued) => queued,
                Err(message) => {
                    return IpcResponse::failure(
                        request.request_id(),
                        vec![IpcDiagnostic::new(
                            "migration_decision_invalid",
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
                            "migration_decision_invalid",
                            message,
                            false,
                        )],
                    );
                }
            };
            if let Err(error) = migration_decisions.enqueue(queued) {
                return IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "migration_decision_queue_unavailable",
                        error.to_string(),
                        true,
                    )],
                );
            }
            let accepted_kind_json = serde_json::to_string(&IpcEventKind::Accepted)
                .expect("accepted event serialization is infallible");
            let operation = DaemonOperationRecord::new(DaemonOperationRecordOptions {
                operation_id: request.request_id().to_owned(),
                kind: "migration_decision".to_owned(),
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
                    drop(migration_decisions.remove(request.request_id()));

                    return IpcResponse::failure(
                        request.request_id(),
                        vec![IpcDiagnostic::new(
                            "migration_decision_queue_unavailable",
                            error.to_string(),
                            true,
                        )],
                    );
                }
            };
            if let Err(error) = event_journal.append_record(accepted) {
                drop(migration_decisions.remove(request.request_id()));

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
        IpcPayload::ProjectRecoveryPoints { canonical_path } => {
            let projects = match control_plane.projects() {
                Ok(projects) => projects,
                Err(error) => {
                    return IpcResponse::failure(
                        request.request_id(),
                        vec![IpcDiagnostic::new(
                            "recovery_points_unavailable",
                            error.to_string(),
                            false,
                        )],
                    );
                }
            };
            let Some(project) = projects
                .iter()
                .find(|project| project.canonical_path() == canonical_path)
            else {
                return IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "project_not_registered",
                        format!(
                            "project path '{}' is not registered",
                            canonical_path.display()
                        ),
                        false,
                    )],
                );
            };
            match control_plane.recovery_points(project.project_name()) {
                Ok(points) => IpcResponse::success(
                    request.request_id(),
                    IpcResult::ProjectRecoveryPoints {
                        recovery_points: points.iter().map(IpcRecoveryPoint::from).collect(),
                    },
                ),
                Err(error) => IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "recovery_points_unavailable",
                        error.to_string(),
                        false,
                    )],
                ),
            }
        }
        IpcPayload::PlanInstallationDeletion => match control_plane.plan_installation_deletion() {
            Ok(plan) => IpcResponse::success(
                request.request_id(),
                IpcResult::InstallationDeletionPlan {
                    plan: IpcInstallationDeletionPlan::from(&plan),
                },
            ),
            Err(message) => IpcResponse::failure(
                request.request_id(),
                vec![IpcDiagnostic::new(
                    "installation_deletion_plan_failed",
                    message,
                    false,
                )],
            ),
        },
        IpcPayload::ExecuteInstallationDeletion { confirmation_token } => {
            match control_plane
                .begin_confirmed_installation_deletion(confirmation_token, now_unix_seconds)
            {
                Ok(_) => match retry_failed_installation_deletion_prune(
                    control_plane,
                    postgres_prunes,
                    event_journal,
                    now_unix_seconds,
                ) {
                    Ok(_) => IpcResponse::success(
                        request.request_id(),
                        IpcResult::InstallationDeletionStarted,
                    ),
                    Err(message) => IpcResponse::failure(
                        request.request_id(),
                        vec![IpcDiagnostic::new(
                            "installation_deletion_retry_failed",
                            message,
                            true,
                        )],
                    ),
                },
                Err(message) => IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "installation_deletion_confirmation_failed",
                        message,
                        false,
                    )],
                ),
            }
        }
        IpcPayload::InstallationDeletionStatus => {
            let status = control_plane
                .installation_lifecycle()
                .map_err(|error| error.to_string())
                .and_then(|lifecycle| {
                    lifecycle.ok_or_else(|| "installation identity is not initialized".to_owned())
                })
                .and_then(|lifecycle| {
                    let logical_resources = control_plane
                        .logical_resources()
                        .map_err(|error| error.to_string())?;
                    let remaining_logical_resources = logical_resources.len();
                    let active_operation_ids = control_plane
                        .active_daemon_operations()
                        .map_err(|error| error.to_string())?
                        .into_iter()
                        .map(|operation| operation.operation_id().to_owned())
                        .collect();
                    let (failed_operation_id, blocking_error) = if lifecycle
                        == crate::control_plane::state::InstallationLifecycle::Deleting
                        && remaining_logical_resources > 0
                    {
                        match control_plane.plan_installation_deletion() {
                            Ok(plan) => {
                                let operation_id =
                                    format!("installation-delete-{}", plan.confirmation_token());
                                let failed = control_plane
                                    .daemon_operation(&operation_id)
                                    .map_err(|error| error.to_string())?
                                    .filter(|operation| {
                                        operation.status() == DaemonOperationStatus::Failed
                                    })
                                    .map(|operation| operation.operation_id().to_owned());
                                (failed, None)
                            }
                            Err(error) => (None, Some(error)),
                        }
                    } else {
                        (None, None)
                    };
                    IpcInstallationDeletionStatus::new(
                        IpcInstallationLifecycle::from(lifecycle),
                        remaining_logical_resources,
                        active_operation_ids,
                        failed_operation_id,
                        blocking_error,
                    )
                });
            match status {
                Ok(status) => IpcResponse::success(
                    request.request_id(),
                    IpcResult::InstallationDeletionStatus { status },
                ),
                Err(message) => IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "installation_deletion_status_failed",
                        message,
                        true,
                    )],
                ),
            }
        }
        IpcPayload::PlanPostgresPrune {
            project_id,
            service_id,
            recovery_point_id,
        } => match plan_postgres_prune(control_plane, project_id, service_id, recovery_point_id) {
            Ok(plan) => {
                IpcResponse::success(request.request_id(), IpcResult::PostgresPrunePlan { plan })
            }
            Err(message) => IpcResponse::failure(
                request.request_id(),
                vec![IpcDiagnostic::new(
                    "postgres_prune_plan_failed",
                    message,
                    false,
                )],
            ),
        },
        IpcPayload::ExecutePostgresPrune {
            project_id,
            service_id,
            recovery_point_id,
            confirmation_token,
        } => {
            let plan = match build_postgres_prune_plan(
                control_plane,
                project_id,
                service_id,
                recovery_point_id,
            ) {
                Ok(plan) => plan,
                Err(message) => {
                    return IpcResponse::failure(
                        request.request_id(),
                        vec![IpcDiagnostic::new(
                            "postgres_prune_confirmation_invalid",
                            message,
                            false,
                        )],
                    );
                }
            };
            let queued = match QueuedPostgresPrune::new(
                request.request_id().to_owned(),
                &plan,
                confirmation_token.clone(),
            ) {
                Ok(queued) => queued,
                Err(message) => {
                    return IpcResponse::failure(
                        request.request_id(),
                        vec![IpcDiagnostic::new(
                            "postgres_prune_confirmation_invalid",
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
                            "postgres_prune_confirmation_invalid",
                            message,
                            false,
                        )],
                    );
                }
            };
            if let Err(error) = postgres_prunes.enqueue(queued) {
                return IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "postgres_prune_queue_unavailable",
                        error.to_string(),
                        true,
                    )],
                );
            }
            let accepted_kind_json = serde_json::to_string(&IpcEventKind::Accepted)
                .expect("accepted event serialization is infallible");
            let operation = DaemonOperationRecord::new(DaemonOperationRecordOptions {
                operation_id: request.request_id().to_owned(),
                kind: "postgres_prune".to_owned(),
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
                    drop(postgres_prunes.remove(request.request_id()));

                    return IpcResponse::failure(
                        request.request_id(),
                        vec![IpcDiagnostic::new(
                            "postgres_prune_queue_unavailable",
                            error.to_string(),
                            true,
                        )],
                    );
                }
            };
            if let Err(error) = event_journal.append_record(accepted) {
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
        IpcPayload::BackupProjectService {
            canonical_path,
            service,
        } => {
            let queued = match prepare_project_backup(
                control_plane,
                request.request_id(),
                canonical_path,
                service,
            ) {
                Ok(queued) => queued,
                Err(message) => {
                    return IpcResponse::failure(
                        request.request_id(),
                        vec![IpcDiagnostic::new("project_backup_invalid", message, false)],
                    );
                }
            };
            let payload_json = match queued.payload_json() {
                Ok(payload) => payload,
                Err(message) => {
                    return IpcResponse::failure(
                        request.request_id(),
                        vec![IpcDiagnostic::new("project_backup_invalid", message, false)],
                    );
                }
            };
            if let Err(error) = project_backups.enqueue(queued) {
                return IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "project_backup_queue_unavailable",
                        error.to_string(),
                        true,
                    )],
                );
            }
            let accepted_kind_json = serde_json::to_string(&IpcEventKind::Accepted)
                .expect("accepted event serialization is infallible");
            let operation = DaemonOperationRecord::new(DaemonOperationRecordOptions {
                operation_id: request.request_id().to_owned(),
                kind: "project_backup".to_owned(),
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
                    drop(project_backups.remove(request.request_id()));

                    return IpcResponse::failure(
                        request.request_id(),
                        vec![IpcDiagnostic::new(
                            "project_backup_queue_unavailable",
                            error.to_string(),
                            true,
                        )],
                    );
                }
            };
            if let Err(error) = event_journal.append_record(accepted) {
                drop(project_backups.remove(request.request_id()));

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
        IpcPayload::RestoreProjectService {
            canonical_path,
            recovery_point_id,
        } => {
            let queued = match prepare_project_restore(
                control_plane,
                request.request_id(),
                canonical_path,
                recovery_point_id,
            ) {
                Ok(queued) => queued,
                Err(message) => {
                    return IpcResponse::failure(
                        request.request_id(),
                        vec![IpcDiagnostic::new(
                            "project_restore_invalid",
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
                            "project_restore_invalid",
                            message,
                            false,
                        )],
                    );
                }
            };
            if let Err(error) = project_restores.enqueue(queued) {
                return IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "project_restore_queue_unavailable",
                        error.to_string(),
                        true,
                    )],
                );
            }
            let accepted_kind_json = serde_json::to_string(&IpcEventKind::Accepted)
                .expect("accepted event serialization is infallible");
            let operation = DaemonOperationRecord::new(DaemonOperationRecordOptions {
                operation_id: request.request_id().to_owned(),
                kind: "project_restore".to_owned(),
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
                    drop(project_restores.remove(request.request_id()));

                    return IpcResponse::failure(
                        request.request_id(),
                        vec![IpcDiagnostic::new(
                            "project_restore_queue_unavailable",
                            error.to_string(),
                            true,
                        )],
                    );
                }
            };
            if let Err(error) = event_journal.append_record(accepted) {
                drop(project_restores.remove(request.request_id()));

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

fn project_migrations<Store>(
    control_plane: &ControlPlane<Store>,
    canonical_path: &std::path::Path,
) -> Result<Vec<IpcMigrationStatus>, String>
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
    let migrations = control_plane
        .migrations()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|migration| migration.project_id() == project.project_name())
        .map(|migration| {
            IpcMigrationStatus::new(
                migration.migration_id().to_owned(),
                migration.phase().label().to_owned(),
                migration.backup_reference().is_some()
                    && migration.backup_artifact_sha256().is_some()
                    && migration.backup_artifact_size_bytes().is_some(),
                migration.phase() == MigrationPhase::Cutover,
                migration.updated_at_unix_seconds(),
            )
        })
        .collect();

    Ok(migrations)
}

fn validate_v7_inventory_path<Store>(
    control_plane: &ControlPlane<Store>,
    canonical_path: &std::path::Path,
) -> Result<(), String>
where
    Store: StateStore,
{
    if !canonical_path.is_absolute() {
        return Err("legacy inventory requires an absolute canonical project path".to_owned());
    }
    let observed_path = std::fs::canonicalize(canonical_path).map_err(|error| {
        format!(
            "legacy project path '{}' cannot be canonicalized: {error}",
            canonical_path.display()
        )
    })?;
    if observed_path != canonical_path {
        return Err(format!(
            "legacy project path '{}' is not canonical; use '{}'",
            canonical_path.display(),
            observed_path.display()
        ));
    }
    let roots = control_plane
        .watched_roots()
        .map_err(|error| error.to_string())?;
    let mut inside_watched_root = false;
    for root in roots {
        let root = std::fs::canonicalize(&root).map_err(|error| {
            format!(
                "watched root '{}' cannot be canonicalized for legacy inventory: {error}",
                root.display()
            )
        })?;
        inside_watched_root |= observed_path.starts_with(root);
    }
    if !inside_watched_root {
        return Err(format!(
            "legacy project '{}' is outside every configured watched root",
            canonical_path.display()
        ));
    }

    Ok(())
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
                let data_lifecycle = match crate::control_plane::retention::resolve_data_lifecycle_strategy(&resource) {
                    Ok(_) => IpcDataLifecycle::LogicalResource,
                    Err(crate::control_plane::retention::DataLifecycleStrategyError::NonAuthoritative { .. }) => {
                        IpcDataLifecycle::None
                    }
                    Err(error) => return Err(error.to_string()),
                };
                let (health, observed_at) = ipc_resource_health(
                    resource_health.observation(resource.shared_resource_id()),
                    now_unix_seconds,
                );
                Ok(IpcResourceStatus::with_data_lifecycle(
                    resource.service_id().to_owned(),
                    resource.kind().to_owned(),
                    ipc_resource_lifecycle(resource.lifecycle()),
                    health,
                    observed_at,
                    true,
                    data_lifecycle,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?,
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

fn prepare_project_backup<Store>(
    control_plane: &ControlPlane<Store>,
    operation_id: &str,
    canonical_path: &std::path::Path,
    service: &str,
) -> Result<QueuedProjectBackup, String>
where
    Store: StateStore,
{
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
    let matches = control_plane
        .logical_resources()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|logical| {
            logical.project_id() == project.project_name()
                && logical.service_id() == service.as_str()
                && logical.lifecycle() == ResourceLifecycle::Active
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [logical] => {
            return QueuedProjectBackup::new(
                operation_id.to_owned(),
                project.project_name().to_owned(),
                service.as_str().to_owned(),
                logical.logical_resource_id().to_owned(),
                logical.kind().to_owned(),
                logical.compatibility_fingerprint().to_owned(),
            );
        }
        [] => {
            let volumes = control_plane
                .resources()
                .map_err(|error| error.to_string())?
                .into_iter()
                .filter(|resource| {
                    resource.project_id() == Some(project.project_name())
                        && resource.scope_id() == Some(service.as_str())
                        && resource.kind() == "volume"
                        && resource.retention() == ResourceRetention::Persistent
                        && resource.lifecycle() == ResourceLifecycle::Active
                })
                .collect::<Vec<_>>();
            match volumes.as_slice() {
                [volume] => {
                    return QueuedProjectBackup::new(
                        operation_id.to_owned(),
                        project.project_name().to_owned(),
                        service.as_str().to_owned(),
                        volume.resource_id().to_owned(),
                        volume.kind().to_owned(),
                        volume.compatibility_fingerprint().to_owned(),
                    );
                }
                [] => {
                    return Err(format!(
                        "project '{}' service '{}' has no active logical data resource or owned persistent volume",
                        project.project_name(),
                        service.as_str()
                    ));
                }
                _ => {
                    return Err(format!(
                        "project '{}' service '{}' has multiple active persistent volumes",
                        project.project_name(),
                        service.as_str()
                    ));
                }
            }
        }
        _ => {
            return Err(format!(
                "project '{}' service '{}' has multiple active logical data resources",
                project.project_name(),
                service.as_str()
            ));
        }
    }
}

fn prepare_migration_decision<Store>(
    control_plane: &ControlPlane<Store>,
    operation_id: &str,
    canonical_path: &std::path::Path,
    migration_id: &str,
    decision: IpcMigrationDecision,
) -> Result<QueuedMigrationDecision, String>
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
    let matches = control_plane
        .migrations()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|migration| {
            migration.migration_id() == migration_id
                && migration.project_id() == project.project_name()
        })
        .collect::<Vec<_>>();
    let migration = match matches.as_slice() {
        [migration] => migration,
        [] => {
            return Err(format!(
                "migration '{migration_id}' does not belong to project '{}'",
                project.project_name()
            ));
        }
        _ => {
            return Err(format!(
                "migration '{migration_id}' matched multiple durable checkpoints"
            ));
        }
    };
    let valid_phase = match decision {
        IpcMigrationDecision::Confirm => matches!(
            migration.phase(),
            MigrationPhase::Cutover | MigrationPhase::Confirmed
        ),
        IpcMigrationDecision::Rollback => matches!(
            migration.phase(),
            MigrationPhase::Cutover | MigrationPhase::RolledBack
        ),
    };
    if !valid_phase {
        return Err(format!(
            "migration '{migration_id}' cannot {} from phase '{}'",
            decision.as_str(),
            migration.phase()
        ));
    }

    QueuedMigrationDecision::new(
        operation_id.to_owned(),
        migration_id.to_owned(),
        project.project_name().to_owned(),
        decision,
    )
}

fn prepare_project_restore<Store>(
    control_plane: &ControlPlane<Store>,
    operation_id: &str,
    canonical_path: &std::path::Path,
    recovery_point_id: &str,
) -> Result<QueuedProjectRestore, String>
where
    Store: StateStore,
{
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
    let recovery_point = control_plane
        .recovery_points(project.project_name())
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|point| point.recovery_point_id() == recovery_point_id)
        .ok_or_else(|| {
            format!(
                "project '{}' has no verified recovery point '{}'",
                project.project_name(),
                recovery_point_id
            )
        })?;
    if recovery_point.resource_kind() == "volume" {
        let resource_matches = control_plane
            .resources()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|resource| {
                resource.project_id() == Some(recovery_point.project_id())
                    && resource.scope_id() == Some(recovery_point.service_id())
                    && resource.resource_id() == recovery_point.logical_resource_id()
                    && resource.kind() == recovery_point.resource_kind()
                    && resource.compatibility_fingerprint()
                        == recovery_point.compatibility_fingerprint()
                    && resource.retention() == ResourceRetention::Persistent
                    && resource.lifecycle() == ResourceLifecycle::Active
            })
            .collect::<Vec<_>>();
        let resource = match resource_matches.as_slice() {
            [resource] => resource,
            [] => {
                return Err(format!(
                    "recovery point '{}' has no exact active persistent volume",
                    recovery_point_id
                ));
            }
            _ => {
                return Err(format!(
                    "recovery point '{}' matches multiple active persistent volumes",
                    recovery_point_id
                ));
            }
        };

        return QueuedProjectRestore::new(QueuedProjectRestoreOptions {
            operation_id: operation_id.to_owned(),
            recovery_point_id: recovery_point.recovery_point_id().to_owned(),
            project_id: resource.project_id().unwrap_or_default().to_owned(),
            service_id: resource.scope_id().unwrap_or_default().to_owned(),
            logical_resource_id: resource.resource_id().to_owned(),
            kind: resource.kind().to_owned(),
            compatibility_fingerprint: resource.compatibility_fingerprint().to_owned(),
        });
    }
    let logical_matches = control_plane
        .logical_resources()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|logical| {
            logical.project_id() == recovery_point.project_id()
                && logical.service_id() == recovery_point.service_id()
                && logical.logical_resource_id() == recovery_point.logical_resource_id()
                && logical.kind() == recovery_point.resource_kind()
                && logical.compatibility_fingerprint() == recovery_point.compatibility_fingerprint()
                && logical.lifecycle() == ResourceLifecycle::Active
        })
        .collect::<Vec<_>>();
    let logical = match logical_matches.as_slice() {
        [logical] => logical,
        [] => {
            return Err(format!(
                "recovery point '{}' has no exact active logical resource",
                recovery_point_id
            ));
        }
        _ => {
            return Err(format!(
                "recovery point '{}' matches multiple active logical resources",
                recovery_point_id
            ));
        }
    };

    QueuedProjectRestore::new(QueuedProjectRestoreOptions {
        operation_id: operation_id.to_owned(),
        recovery_point_id: recovery_point.recovery_point_id().to_owned(),
        project_id: logical.project_id().to_owned(),
        service_id: logical.service_id().to_owned(),
        logical_resource_id: logical.logical_resource_id().to_owned(),
        kind: logical.kind().to_owned(),
        compatibility_fingerprint: logical.compatibility_fingerprint().to_owned(),
    })
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
