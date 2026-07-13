use super::ProjectCommandPlan;
use crate::control_plane::engine::{
    CommandExecutor, EngineError, OwnedContainer, ResourceKind, RetentionClass,
    run_attached_command,
};

/// Executes one project tool or hook only inside its owned application runtime.
pub(crate) async fn run_project_command(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    plan: &ProjectCommandPlan,
) -> Result<(), EngineError> {
    let metadata = container.metadata();
    if metadata.kind() != ResourceKind::ProjectApplication
        || metadata.retention() != RetentionClass::Disposable
        || metadata.project_id() != Some(plan.project_id())
    {
        return Err(EngineError::InvalidRequest {
            detail: format!(
                "project command for '{}' cannot execute in project application '{}'",
                plan.project_id(),
                metadata.project_id().unwrap_or("unowned")
            ),
        });
    }

    run_attached_command(executor, container, plan.attached()).await
}
