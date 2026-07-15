use super::{
    ProjectVolumeReconcileResult, ProjectVolumesReconcileOptions, WorkloadReconcileError,
    execute_project_volume_reconciliation, plan_project_volume_reconciliation,
};
use crate::control_plane::engine::VolumeManager;
use futures_util::stream::{self, StreamExt};

/// Preflights the complete batch, then converges independent volumes boundedly.
pub(crate) async fn reconcile_project_volumes_from_observed<Engine>(
    engine: &Engine,
    options: ProjectVolumesReconcileOptions<'_>,
) -> Result<Vec<Result<ProjectVolumeReconcileResult, WorkloadReconcileError>>, WorkloadReconcileError>
where
    Engine: Clone + VolumeManager,
{
    let mut plans = Vec::with_capacity(options.requests.len());
    for request in options.requests {
        match plan_project_volume_reconciliation(
            request,
            options.observed,
            options.installation_id,
            options.schema_version,
        ) {
            Ok(plan) => plans.push(Ok(plan)),
            Err(error @ WorkloadReconcileError::DestructiveReplacementRequired { .. }) => {
                plans.push(Err(error));
            }
            Err(error) => return Err(error),
        }
    }

    Ok(stream::iter(plans.into_iter().map(|plan| {
        let mut engine = engine.clone();

        async move {
            match plan {
                Ok(plan) => execute_project_volume_reconciliation(&mut engine, plan).await,
                Err(error) => Err(error),
            }
        }
    }))
    .buffered(options.concurrency.get())
    .collect::<Vec<_>>()
    .await)
}
