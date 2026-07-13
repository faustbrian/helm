use super::WorkloadPlanError;

pub(super) fn validate_image_digest(workload: &str, image: &str) -> Result<(), WorkloadPlanError> {
    let valid = image.rsplit_once("@sha256:").is_some_and(|(_, digest)| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    });

    if !valid {
        return Err(WorkloadPlanError::new(format!(
            "{workload} image '{image}' must use an immutable sha256 digest"
        )));
    }

    Ok(())
}
