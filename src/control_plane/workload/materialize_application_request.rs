use super::{ImmutableProjectApplicationPlan, WorkloadReconcileError};
use crate::control_plane::engine::{ContainerCreateOptions, EngineError, ImageBuilder};

/// Resolves an optional derived runtime before its application can be mutated.
pub(crate) async fn materialize_application_request<E>(
    engine: &E,
    application: &ImmutableProjectApplicationPlan,
) -> Result<ContainerCreateOptions, WorkloadReconcileError>
where
    E: ImageBuilder,
{
    let Some(runtime_image) = application.runtime_image() else {
        return Ok(application.request().clone());
    };
    let image = engine
        .build_image(runtime_image.request())
        .await
        .map_err(engine_error)?;

    application
        .request()
        .clone()
        .with_image(image.as_str())
        .map_err(invalid_request)
}

fn engine_error(error: EngineError) -> WorkloadReconcileError {
    WorkloadReconcileError::Engine {
        action: "build application runtime image".to_owned(),
        detail: error.to_string(),
    }
}

fn invalid_request(error: EngineError) -> WorkloadReconcileError {
    WorkloadReconcileError::InvalidRequest {
        detail: error.to_string(),
    }
}
