use super::WorkloadReconcileError;
use crate::control_plane::engine::{
    ContainerCreateOptions, ImageResolver, ImmutableImageReference,
};

/// Materializes one dedicated service's exact locked image before mutation.
pub(super) async fn ensure_project_service_image<Engine>(
    engine: &mut Engine,
    request: &ContainerCreateOptions,
) -> Result<(), WorkloadReconcileError>
where
    Engine: ImageResolver,
{
    let reference = ImmutableImageReference::new(request.image().to_owned()).map_err(|error| {
        WorkloadReconcileError::InvalidRequest {
            detail: error.to_string(),
        }
    })?;

    engine
        .ensure_image(&reference)
        .await
        .map(|_| ())
        .map_err(|error| WorkloadReconcileError::Engine {
            action: "resolve dedicated service image".to_owned(),
            detail: error.to_string(),
        })
}
