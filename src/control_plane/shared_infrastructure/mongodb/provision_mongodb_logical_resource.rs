use super::MongoDbLogicalResourcePlan;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, OwnedContainer,
    run_attached_command,
};
use std::collections::BTreeMap;
use std::time::Duration;

const PROVISIONING_TIMEOUT_SECONDS: u64 = 30;

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

    run_attached_command(executor, container, &options).await
}
