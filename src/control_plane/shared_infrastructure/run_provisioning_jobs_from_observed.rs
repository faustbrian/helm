use super::{
    ProvisioningJobOptions, ProvisioningJobsRunOptions, SharedInfrastructureReconcileError,
    run_provisioning_job_from_observed,
};
use crate::control_plane::engine::{
    ContainerCompletion, ContainerCreateOptions, ContainerLifecycle, ImageResolver,
};
use futures_util::stream::{self, StreamExt};

/// Runs independent provisioning jobs boundedly in desired-plan order.
pub(crate) async fn run_provisioning_jobs_from_observed<Engine>(
    engine: &Engine,
    options: ProvisioningJobsRunOptions<'_>,
) -> Vec<(
    ContainerCreateOptions,
    Result<(), SharedInfrastructureReconcileError>,
)>
where
    Engine: Clone + ContainerCompletion + ContainerLifecycle + ImageResolver,
{
    stream::iter(options.requests.iter().cloned().map(|request| {
        let mut engine = engine.clone();

        async move {
            let result = run_provisioning_job_from_observed(
                &mut engine,
                options.observed,
                ProvisioningJobOptions {
                    request: &request,
                    installation_id: options.installation_id,
                    schema_version: options.schema_version,
                    timeout: options.timeout,
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
