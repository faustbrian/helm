use super::{POSTGRES_BOOTSTRAP_USERNAME, PostgresLogicalResourcePlan};
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, OwnedContainer,
    run_attached_command,
};
use crate::control_plane::state::{CredentialLifecycle, CredentialRecord};
use std::collections::BTreeMap;
use std::time::Duration;

const PROVISIONING_TIMEOUT_SECONDS: u64 = 30;

/// Applies one idempotent database/role plan through attached Engine exec.
pub(crate) async fn provision_postgres_logical_resource(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    plan: &PostgresLogicalResourcePlan,
    administrator: &CredentialRecord,
) -> Result<(), EngineError> {
    if administrator.username() != POSTGRES_BOOTSTRAP_USERNAME
        || administrator.secret().is_empty()
        || administrator.lifecycle() != CredentialLifecycle::Active
    {
        return Err(EngineError::InvalidRequest {
            detail: "PostgreSQL provisioning requires the active bootstrap administrator"
                .to_owned(),
        });
    }
    let request = CommandRequest::new(
        plan.command_arguments().to_vec(),
        BTreeMap::from([("PGPASSWORD".to_owned(), administrator.secret().to_owned())]),
        None,
    )?;
    let options = AttachedCommandOptions::new(
        request,
        plan.stdin_sql().as_bytes().to_vec(),
        "provision PostgreSQL logical resource",
        Duration::from_secs(PROVISIONING_TIMEOUT_SECONDS),
    )?;

    run_attached_command(executor, container, &options).await
}
