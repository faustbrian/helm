use super::{PreparedSharedInstance, SharedInfrastructureReconcileError};
use crate::control_plane::engine::{ImageResolver, ImmutableImageReference};

/// Materializes the exact locked image before any shared-service mutation.
pub(super) async fn ensure_prepared_shared_image<Engine>(
    engine: &mut Engine,
    prepared: &PreparedSharedInstance,
) -> Result<(), SharedInfrastructureReconcileError>
where
    Engine: ImageResolver,
{
    let image = prepared.container_request().image();
    let reference = ImmutableImageReference::new(image.to_owned()).map_err(|error| {
        SharedInfrastructureReconcileError::InvalidRequest {
            detail: error.to_string(),
        }
    })?;

    engine
        .ensure_image(&reference)
        .await
        .map(|_| ())
        .map_err(|error| SharedInfrastructureReconcileError::Engine {
            action: "image resolution".to_owned(),
            detail: error.to_string(),
        })
}
