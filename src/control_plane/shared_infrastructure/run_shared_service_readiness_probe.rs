use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, EngineError, OwnedContainer, run_attached_command,
};
use std::time::Duration;

const READINESS_ATTEMPTS: usize = 60;
const READINESS_RETRY_MILLISECONDS: u64 = 500;

/// Retries one bounded authenticated shared-service probe during startup.
pub(crate) async fn run_shared_service_readiness_probe(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &AttachedCommandOptions,
) -> Result<(), EngineError> {
    for attempt in 1..=READINESS_ATTEMPTS {
        match run_attached_command(executor, container, options).await {
            Ok(()) => return Ok(()),
            Err(EngineError::ContainerExit { .. } | EngineError::Backend { .. })
                if attempt < READINESS_ATTEMPTS =>
            {
                tokio::time::sleep(Duration::from_millis(READINESS_RETRY_MILLISECONDS)).await;
            }
            Err(error) => return Err(error),
        }
    }

    unreachable!("the final shared-service readiness attempt always returns")
}
