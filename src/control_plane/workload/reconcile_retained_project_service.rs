use super::reconcile_project_application::reconcile_project_workload;
use super::{WorkloadReconcileError, WorkloadReconcileOptions, WorkloadReconcileResult};
use crate::control_plane::engine::{
    ContainerDiscovery, ContainerLifecycle, HealthObserver, ResourceKind, RetentionClass,
};

/// Restores one retained project service without weakening normal workloads.
pub(crate) async fn reconcile_retained_project_service<E>(
    engine: &mut E,
    options: WorkloadReconcileOptions<'_>,
) -> Result<WorkloadReconcileResult, WorkloadReconcileError>
where
    E: ContainerDiscovery + ContainerLifecycle + HealthObserver,
{
    reconcile_project_workload(
        engine,
        options,
        ResourceKind::ProjectService,
        RetentionClass::Persistent,
    )
    .await
}
