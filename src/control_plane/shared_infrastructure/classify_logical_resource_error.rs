use super::SharedInfrastructureReconcileError;
use crate::control_plane::engine::EngineError;

/// Preserves logical command rejection without treating the Engine as lost.
pub(crate) fn classify_logical_resource_error(
    resource_id: impl Into<String>,
    action: impl Into<String>,
    source: EngineError,
) -> SharedInfrastructureReconcileError {
    let resource_id = resource_id.into();
    let action = action.into();

    match source {
        EngineError::ContainerExit { status_code, .. } => {
            SharedInfrastructureReconcileError::LogicalResourceDrift {
                resource_id,
                detail: format!("{action} exited with status {status_code}"),
            }
        }
        engine_error => SharedInfrastructureReconcileError::Engine {
            action,
            detail: engine_error.to_string(),
        },
    }
}
