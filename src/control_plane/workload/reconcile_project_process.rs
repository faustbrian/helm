use super::reconcile_project_application::reconcile_project_workload;
use super::{WorkloadReconcileError, WorkloadReconcileOptions, WorkloadReconcileResult};
use crate::control_plane::engine::{
    ContainerDiscovery, ContainerLifecycle, HealthObserver, ResourceKind,
};

/// Restores one supervised project worker or scheduler by stable resource identity.
pub(crate) async fn reconcile_project_process<E>(
    engine: &mut E,
    options: WorkloadReconcileOptions<'_>,
) -> Result<WorkloadReconcileResult, WorkloadReconcileError>
where
    E: ContainerDiscovery + ContainerLifecycle + HealthObserver,
{
    reconcile_project_workload(engine, options, ResourceKind::ProjectProcess).await
}
