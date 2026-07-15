use super::{
    ProjectVolumeReconcileAction, ProjectVolumeReconcilePlan, ProjectVolumeReconcileResult,
    WorkloadReconcileError,
};
use crate::control_plane::engine::{EngineError, VolumeManager};

/// Executes one fully preflighted retained-volume plan.
pub(crate) async fn execute_project_volume_reconciliation<Engine>(
    engine: &mut Engine,
    plan: ProjectVolumeReconcilePlan,
) -> Result<ProjectVolumeReconcileResult, WorkloadReconcileError>
where
    Engine: VolumeManager,
{
    match plan {
        ProjectVolumeReconcilePlan::Create(request) => {
            let volume = engine
                .create_volume(&request)
                .await
                .map_err(|error| engine_error("create project volume", error))?;
            if volume.name() != request.name() || volume.metadata() != request.metadata() {
                return Err(WorkloadReconcileError::Engine {
                    action: "create project volume".to_owned(),
                    detail: "Engine returned a project volume with unexpected ownership".to_owned(),
                });
            }

            Ok(ProjectVolumeReconcileResult::new(
                volume,
                ProjectVolumeReconcileAction::Created,
            ))
        }
        ProjectVolumeReconcilePlan::Adopt(volume) => Ok(ProjectVolumeReconcileResult::new(
            volume,
            ProjectVolumeReconcileAction::Unchanged,
        )),
    }
}

fn engine_error(action: &str, error: EngineError) -> WorkloadReconcileError {
    WorkloadReconcileError::Engine {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}
