use super::EngineError;

/// Rejects a completed container when its process did not exit successfully.
pub(crate) fn validate_container_completion(
    container_id: &str,
    status_code: i64,
) -> Result<(), EngineError> {
    if status_code == 0 {
        return Ok(());
    }

    Err(EngineError::ContainerExit {
        container_id: container_id.to_owned(),
        status_code,
    })
}
