use super::MongoDbLogicalResourcePlan;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, OwnedContainer,
    run_attached_command,
};
use std::collections::BTreeMap;
use std::time::Duration;

const PROVISIONING_TIMEOUT_SECONDS: u64 = 30;
const CONNECTION_RETRY_ATTEMPTS: usize = 20;
const CONNECTION_RETRY_MILLISECONDS: u64 = 250;

/// Applies one MongoDB database-user plan through attached Engine exec.
pub(crate) async fn provision_mongodb_logical_resource(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    plan: &MongoDbLogicalResourcePlan,
) -> Result<(), EngineError> {
    let request = CommandRequest::new(
        plan.command_arguments()
            .into_iter()
            .map(str::to_owned)
            .collect(),
        BTreeMap::new(),
        None,
    )?;
    let options = AttachedCommandOptions::new(
        request,
        plan.stdin_script().as_bytes().to_vec(),
        "provision MongoDB logical resource",
        Duration::from_secs(PROVISIONING_TIMEOUT_SECONDS),
    )?;

    for attempt in 1..=CONNECTION_RETRY_ATTEMPTS {
        match run_attached_command(executor, container, &options).await {
            Ok(()) => return Ok(()),
            Err(EngineError::ContainerExit { status_code: 1, .. })
                if attempt < CONNECTION_RETRY_ATTEMPTS =>
            {
                tokio::time::sleep(Duration::from_millis(CONNECTION_RETRY_MILLISECONDS)).await;
            }
            Err(error) => return Err(error),
        }
    }

    unreachable!("the final MongoDB provisioning attempt always returns")
}
