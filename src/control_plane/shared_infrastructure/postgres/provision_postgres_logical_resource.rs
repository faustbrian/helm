use super::PostgresLogicalResourcePlan;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, OwnedContainer,
    run_attached_command,
};
use std::collections::BTreeMap;
use std::time::Duration;

const PROVISIONING_TIMEOUT_SECONDS: u64 = 30;

/// Applies one idempotent database/role plan through attached Engine exec.
pub(crate) async fn provision_postgres_logical_resource(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    plan: &PostgresLogicalResourcePlan,
) -> Result<(), EngineError> {
    let request = CommandRequest::new(plan.command_arguments().to_vec(), BTreeMap::new(), None)?;
    let options = AttachedCommandOptions::new(
        request,
        plan.stdin_sql().as_bytes().to_vec(),
        "provision PostgreSQL logical resource",
        Duration::from_secs(PROVISIONING_TIMEOUT_SECONDS),
    )?;

    run_attached_command(executor, container, &options).await
}
