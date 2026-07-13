use super::record_ipc_event::record_ipc_event;
use super::{DaemonRequestDispatchOptions, reconcile_watched_roots};
use crate::control_plane::daemon::ipc::{
    IpcDiagnostic, IpcEventKind, IpcPayload, IpcResponse, IpcResult,
};
use crate::control_plane::state::StateStore;

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
