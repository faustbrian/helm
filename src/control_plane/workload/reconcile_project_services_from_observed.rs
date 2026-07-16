use super::{
    ProjectServicesReconcileOptions, WorkloadReconcileError, WorkloadReconcileOptions,
    WorkloadReconcileResult, reconcile_project_service_from_observed,
};
use crate::control_plane::engine::{
    ContainerCreateOptions, ContainerLifecycle, HealthObserver, ImageResolver,
};
use futures_util::stream::{self, StreamExt};

/// Converges independent dedicated services boundedly in desired-plan order.
pub(crate) async fn reconcile_project_services_from_observed<Engine>(
    engine: &Engine,
    options: ProjectServicesReconcileOptions<'_>,
) -> Vec<(
    ContainerCreateOptions,
    Result<WorkloadReconcileResult, WorkloadReconcileError>,
)>
where
    Engine: Clone + ContainerLifecycle + HealthObserver + ImageResolver,
{
    stream::iter(options.requests.iter().cloned().map(|request| {
        let mut engine = engine.clone();

        async move {
            let result = reconcile_project_service_from_observed(
                &mut engine,
                options.observed,
                WorkloadReconcileOptions {
                    request: &request,
                    installation_id: options.installation_id,
                    schema_version: options.schema_version,
                },
            )
            .await;

            (request, result)
        }
    }))
    .buffered(options.concurrency.get())
    .collect()
    .await
}
