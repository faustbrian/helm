use super::ensure_project_service_image::ensure_project_service_image;
#[cfg(test)]
use super::reconcile_project_application::reconcile_project_workload;
use super::reconcile_project_application::reconcile_project_workload_from_observed;
use super::{WorkloadReconcileError, WorkloadReconcileOptions, WorkloadReconcileResult};
#[cfg(test)]
use crate::control_plane::engine::ContainerDiscovery;
use crate::control_plane::engine::{
    ContainerLifecycle, HealthObserver, ImageResolver, ObservedContainer, ResourceKind,
    RetentionClass,
};

/// Restores one isolated project infrastructure service by exact identity.
#[cfg(test)]
pub(crate) async fn reconcile_project_service<E>(
    engine: &mut E,
    options: WorkloadReconcileOptions<'_>,
) -> Result<WorkloadReconcileResult, WorkloadReconcileError>
where
    E: ContainerDiscovery + ContainerLifecycle + HealthObserver + ImageResolver,
{
    ensure_project_service_image(engine, options.request).await?;

    reconcile_project_workload(
        engine,
        options,
        ResourceKind::ProjectService,
        RetentionClass::Disposable,
    )
    .await
}

/// Reconciles one isolated service against a pass-wide Engine observation.
pub(crate) async fn reconcile_project_service_from_observed<E>(
    engine: &mut E,
    observed: &[ObservedContainer],
    options: WorkloadReconcileOptions<'_>,
) -> Result<WorkloadReconcileResult, WorkloadReconcileError>
where
    E: ContainerLifecycle + HealthObserver + ImageResolver,
{
    ensure_project_service_image(engine, options.request).await?;

    reconcile_project_workload_from_observed(
        engine,
        observed,
        options,
        ResourceKind::ProjectService,
        RetentionClass::Disposable,
    )
    .await
}
