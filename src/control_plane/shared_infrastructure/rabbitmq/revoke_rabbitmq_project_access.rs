use super::RabbitMqProjectDefinition;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, OwnedContainer,
    run_attached_command, run_attached_command_capture,
};
use std::collections::BTreeMap;
use std::time::Duration;

const RABBITMQ_CONTROL_TIMEOUT_SECONDS: u64 = 30;

/// Revokes one project user while deliberately retaining its vhost and data.
pub(crate) async fn revoke_rabbitmq_project_access(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    definition: &RabbitMqProjectDefinition,
) -> Result<bool, EngineError> {
    let list = command_options(
        vec![
            "rabbitmqctl".to_owned(),
            "list_users".to_owned(),
            "name".to_owned(),
            "--no-table-headers".to_owned(),
        ],
        "list RabbitMQ users before access revocation",
    )?;
    let output = run_attached_command_capture(executor, container, &list).await?;
    let users = std::str::from_utf8(&output).map_err(|error| EngineError::Backend {
        detail: format!("RabbitMQ user list was not valid UTF-8: {error}"),
    })?;
    if !users
        .lines()
        .map(str::trim)
        .any(|username| username == definition.username())
    {
        return Ok(false);
    }

    let delete = command_options(
        vec![
            "rabbitmqctl".to_owned(),
            "delete_user".to_owned(),
            definition.username().to_owned(),
        ],
        "revoke RabbitMQ project access",
    )?;
    run_attached_command(executor, container, &delete).await?;

    Ok(true)
}

fn command_options(
    arguments: Vec<String>,
    action: &str,
) -> Result<AttachedCommandOptions, EngineError> {
    let request = CommandRequest::new(arguments, BTreeMap::new(), None)?;

    AttachedCommandOptions::new(
        request,
        Vec::new(),
        action,
        Duration::from_secs(RABBITMQ_CONTROL_TIMEOUT_SECONDS),
    )
}
