use super::WorkloadPlanError;
use crate::control_plane::engine::is_immutable_image_identity;

pub(super) fn validate_image_digest(workload: &str, image: &str) -> Result<(), WorkloadPlanError> {
    if !is_immutable_image_identity(image) {
        return Err(WorkloadPlanError::new(format!(
            "{workload} image '{image}' must use an immutable sha256 digest"
        )));
    }

    Ok(())
}
