use super::{HostCommandOutput, TrustStoreError};

pub(super) fn require_host_command_success(
    action: &str,
    output: &HostCommandOutput,
) -> Result<(), TrustStoreError> {
    if output.succeeded() {
        return Ok(());
    }

    let detail = output.stderr().trim();
    let suffix = if detail.is_empty() {
        String::new()
    } else {
        format!(": {detail}")
    };

    Err(TrustStoreError::new(format!("failed to {action}{suffix}")))
}
