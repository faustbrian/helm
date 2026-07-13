use super::{HostCommand, HostCommandOutput, TrustStoreError};

/// Replaceable executor for narrow operating-system trust commands.
pub(crate) trait HostCommandExecutor {
    fn execute(&self, command: &HostCommand) -> Result<HostCommandOutput, TrustStoreError>;
}
