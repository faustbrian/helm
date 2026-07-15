use super::materialize_application_request;
use super::{ImmutableProjectApplicationPlan, WorkloadReconcileError};
use crate::control_plane::engine::{
    ContainerCreateOptions, EngineError, ImageBuilder, ImageResolver,
};
use std::collections::BTreeMap;

/// Resolves one reconciliation pass without repeating equal runtime work.
pub(crate) async fn materialize_application_requests<E>(
    engine: &mut E,
    applications: &[ImmutableProjectApplicationPlan],
) -> Result<Vec<ContainerCreateOptions>, WorkloadReconcileError>
where
    E: ImageBuilder + ImageResolver,
{
    let mut materialized = Vec::with_capacity(applications.len());
    let mut runtime_images = BTreeMap::new();

    for application in applications {
        let Some(runtime) = application.runtime_image() else {
            materialized.push(application.request().clone());

            continue;
        };
        let input_digest = runtime.request().input_digest();
        if let Some(image) = runtime_images.get(input_digest) {
            materialized.push(
                application
                    .request()
                    .clone()
                    .with_image(image)
                    .map_err(invalid_request)?,
            );

            continue;
        }
        let request = materialize_application_request(engine, application).await?;
        runtime_images.insert(input_digest.to_owned(), request.image().to_owned());
        materialized.push(request);
    }

    Ok(materialized)
}

fn invalid_request(error: EngineError) -> WorkloadReconcileError {
    WorkloadReconcileError::InvalidRequest {
        detail: error.to_string(),
    }
}
