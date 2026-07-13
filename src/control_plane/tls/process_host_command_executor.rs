use super::{HostCommand, HostCommandExecutor, HostCommandOutput, TrustStoreError};
use std::process::Command;

/// Standard-process implementation for explicitly allowed OS trust commands.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ProcessHostCommandExecutor;

impl HostCommandExecutor for ProcessHostCommandExecutor {
    fn execute(&self, command: &HostCommand) -> Result<HostCommandOutput, TrustStoreError> {
        let output = Command::new(command.program())
            .args(command.arguments())
            .output()
            .map_err(|error| {
                TrustStoreError::new(format!(
                    "failed to execute host trust command '{}': {error}",
                    command.program()
                ))
            })?;

        Ok(HostCommandOutput::from_process(
            output.status.success(),
            output.stdout,
            output.stderr,
        ))
    }
}
