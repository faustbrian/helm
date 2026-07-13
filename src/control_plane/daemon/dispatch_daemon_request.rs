use super::{ProjectDiscoveryOptions, reconcile_watched_roots};
use crate::control_plane::application::ControlPlane;
use crate::control_plane::daemon::ipc::{
    IpcDiagnostic, IpcPayload, IpcRequest, IpcResponse, IpcResult,
};
use crate::control_plane::state::StateStore;

/// Dispatches one correlated request without allowing partial registry mutation.
pub(crate) fn dispatch_daemon_request<Store>(
    control_plane: &mut ControlPlane<Store>,
    discovery_options: ProjectDiscoveryOptions,
    request: &IpcRequest,
    now_unix_seconds: i64,
) -> IpcResponse
where
    Store: StateStore,
{
    match request.payload() {
        IpcPayload::Ping => IpcResponse::success(request.request_id(), IpcResult::Pong),
        IpcPayload::Reconcile => {
            match reconcile_watched_roots(control_plane, discovery_options, now_unix_seconds) {
                Ok(result) => IpcResponse::success(
                    request.request_id(),
                    IpcResult::Reconciled {
                        project_count: result.report().sources().len(),
                        issue_count: result.report().issues().len(),
                        applied: result.was_applied(),
                    },
                ),
                Err(error) => IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "reconciliation_failed",
                        error.to_string(),
                        true,
                    )],
                ),
            }
        }
        IpcPayload::AdoptProject { canonical_path } => {
            match control_plane.adopt_project(canonical_path) {
                Ok(project_id) => IpcResponse::success(
                    request.request_id(),
                    IpcResult::ProjectAdopted { project_id },
                ),
                Err(error) => IpcResponse::failure(
                    request.request_id(),
                    vec![IpcDiagnostic::new(
                        "project_adoption_failed",
                        error.to_string(),
                        false,
                    )],
                ),
            }
        }
        IpcPayload::Cancel { .. } | IpcPayload::SubscribeEvents { .. } => IpcResponse::failure(
            request.request_id(),
            vec![IpcDiagnostic::new(
                "operation_not_available",
                "the singleton daemon does not support this operation yet",
                false,
            )],
        ),
    }
}
