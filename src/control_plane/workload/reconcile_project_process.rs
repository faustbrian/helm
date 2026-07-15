#[cfg(test)]
use super::reconcile_project_application::reconcile_project_workload;
use super::reconcile_project_application::reconcile_project_workload_from_observed;
use super::{WorkloadReconcileError, WorkloadReconcileOptions, WorkloadReconcileResult};
#[cfg(test)]
use crate::control_plane::engine::ContainerDiscovery;
use crate::control_plane::engine::{
    ContainerLifecycle, HealthObserver, ObservedContainer, ResourceKind, RetentionClass,
};

/// Restores one supervised project worker by stable resource identity.
#[cfg(test)]
pub(crate) async fn reconcile_project_process<E>(
    engine: &mut E,
    options: WorkloadReconcileOptions<'_>,
) -> Result<WorkloadReconcileResult, WorkloadReconcileError>
where
    E: ContainerDiscovery + ContainerLifecycle + HealthObserver,
{
    reconcile_project_workload(
        engine,
        options,
        ResourceKind::ProjectProcess,
        RetentionClass::Disposable,
    )
    .await
}

/// Reconciles one supervised process against a pass-wide Engine observation.
pub(crate) async fn reconcile_project_process_from_observed<E>(
    engine: &mut E,
    observed: &[ObservedContainer],
    options: WorkloadReconcileOptions<'_>,
) -> Result<WorkloadReconcileResult, WorkloadReconcileError>
where
    E: ContainerLifecycle + HealthObserver,
{
    reconcile_project_workload_from_observed(
        engine,
        observed,
        options,
        ResourceKind::ProjectProcess,
        RetentionClass::Disposable,
    )
    .await
}
