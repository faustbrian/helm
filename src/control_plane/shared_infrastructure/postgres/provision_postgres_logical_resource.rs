use super::PostgresLogicalResourcePlan;
use crate::control_plane::engine::{
    CommandExecutor, CommandRequest, CommandStatus, EngineError, OwnedContainer,
};
use futures_util::StreamExt;
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

const PROVISIONING_TIMEOUT_SECONDS: u64 = 30;
const STATUS_POLL_MILLISECONDS: u64 = 20;

/// Applies one idempotent database/role plan through attached Engine exec.
pub(crate) async fn provision_postgres_logical_resource(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    plan: &PostgresLogicalResourcePlan,
) -> Result<(), EngineError> {
    tokio::time::timeout(
        Duration::from_secs(PROVISIONING_TIMEOUT_SECONDS),
        provision(executor, container, plan),
    )
    .await
    .map_err(|_| EngineError::Timeout {
        action: "provision PostgreSQL logical resource".to_owned(),
        timeout_milliseconds: PROVISIONING_TIMEOUT_SECONDS * 1_000,
    })?
}

async fn provision(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    plan: &PostgresLogicalResourcePlan,
) -> Result<(), EngineError> {
    let request = CommandRequest::new(plan.command_arguments().to_vec(), BTreeMap::new(), None)?;
    let session = executor.start_command(container, &request).await?;
    let (execution_id, container_id, mut input, mut output) = session.into_parts();

    input
        .write_all(plan.stdin_sql().as_bytes())
        .await
        .map_err(|error| EngineError::Backend {
            detail: format!("failed to write PostgreSQL provisioning input: {error}"),
        })?;
    input
        .shutdown()
        .await
        .map_err(|error| EngineError::Backend {
            detail: format!("failed to close PostgreSQL provisioning input: {error}"),
        })?;
    drop(input);

    while let Some(chunk) = output.next().await {
        chunk?;
    }

    loop {
        match executor
            .command_status(&execution_id, &container_id)
            .await?
        {
            CommandStatus::Running => {
                tokio::time::sleep(Duration::from_millis(STATUS_POLL_MILLISECONDS)).await;
            }
            CommandStatus::Exited(0) => return Ok(()),
            CommandStatus::Exited(status) => {
                return Err(EngineError::Backend {
                    detail: format!(
                        "PostgreSQL logical-resource provisioning exited with status {status}"
                    ),
                });
            }
        }
    }
}
