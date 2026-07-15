use super::{
    ProjectVolumeReconcileOptions, ProjectVolumeReconcileResult, WorkloadReconcileError,
    execute_project_volume_reconciliation, plan_project_volume_reconciliation,
};
use crate::control_plane::engine::{EngineError, ObservedVolume, VolumeDiscovery, VolumeManager};

/// Creates or adopts one exact retained project volume without deleting data.
pub(crate) async fn reconcile_project_volume<E>(
    engine: &mut E,
    options: ProjectVolumeReconcileOptions<'_>,
) -> Result<ProjectVolumeReconcileResult, WorkloadReconcileError>
where
    E: VolumeDiscovery + VolumeManager,
{
    let observed = engine
        .discover_managed_volumes()
        .await
        .map_err(|error| engine_error("discover project volumes", error))?;

    reconcile_project_volume_from_observed(engine, &observed, options).await
}

/// Reconciles one retained volume against a pass-wide Engine observation.
pub(crate) async fn reconcile_project_volume_from_observed<E>(
    engine: &mut E,
    observed: &[ObservedVolume],
    options: ProjectVolumeReconcileOptions<'_>,
) -> Result<ProjectVolumeReconcileResult, WorkloadReconcileError>
where
    E: VolumeManager,
{
    let plan = plan_project_volume_reconciliation(
        options.request,
        observed,
        options.installation_id,
        options.schema_version,
    )?;

    execute_project_volume_reconciliation(engine, plan).await
}

fn engine_error(action: &str, error: EngineError) -> WorkloadReconcileError {
    WorkloadReconcileError::Engine {
        action: action.to_owned(),
        detail: error.to_string(),
    }
}
