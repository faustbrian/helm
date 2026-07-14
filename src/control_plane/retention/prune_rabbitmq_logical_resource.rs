use super::RabbitMqLogicalPruneOptions;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, run_attached_command,
    run_attached_command_capture,
};
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use std::collections::BTreeMap;

/// Idempotently revokes one user before deleting only its exact vhost.
pub(crate) async fn prune_rabbitmq_logical_resource(
    executor: &impl CommandExecutor,
    options: RabbitMqLogicalPruneOptions<'_>,
) -> Result<(), EngineError> {
    let (username, vhost) = validate(&options)?;
    if listed(
        executor,
        options.container,
        vec![
            "rabbitmqctl".to_owned(),
            "list_users".to_owned(),
            "name".to_owned(),
            "--no-table-headers".to_owned(),
        ],
        username,
        "list RabbitMQ users before logical prune",
        options.timeout,
    )
    .await?
    {
        run(
            executor,
            options.container,
            vec![
                "rabbitmqctl".to_owned(),
                "delete_user".to_owned(),
                username.to_owned(),
            ],
            "revoke confirmed RabbitMQ tenant user",
            options.timeout,
        )
        .await?;
    }
    if listed(
        executor,
        options.container,
        vec![
            "rabbitmqctl".to_owned(),
            "list_vhosts".to_owned(),
            "name".to_owned(),
            "--no-table-headers".to_owned(),
        ],
        &vhost,
        "list RabbitMQ vhosts before logical prune",
        options.timeout,
    )
    .await?
    {
        run(
            executor,
            options.container,
            vec!["rabbitmqctl".to_owned(), "delete_vhost".to_owned(), vhost],
            "delete confirmed RabbitMQ tenant vhost",
            options.timeout,
        )
        .await?;
    }

    Ok(())
}

fn validate<'operation>(
    options: &'operation RabbitMqLogicalPruneOptions<'operation>,
) -> Result<(&'operation str, String), EngineError> {
    let logical = options.logical_resource;
    let credential = options.credential;
    let identity = format!(
        "{}_{}",
        logical.project_id().replace('-', "_"),
        logical.service_id().replace('-', "_")
    );
    let expected_username = format!("st_{identity}");
    let vhost = format!("stackctl_{identity}");
    let invalid = options.installation_id.is_empty()
        || options.timeout.is_zero()
        || options.container.metadata().installation_id() != options.installation_id
        || options.container.metadata().compatibility_fingerprint()
            != logical.compatibility_fingerprint()
        || logical.kind() != "rabbitmq_vhost_user"
        || logical.lifecycle() == ResourceLifecycle::Active
        || logical.orphaned_at_unix_seconds().is_none()
        || logical.logical_resource_id() != credential.credential_id()
        || credential.project_id() != Some(logical.project_id())
        || credential.service_id() != logical.service_id()
        || credential.username() != expected_username
        || credential.lifecycle() != CredentialLifecycle::Disabled
        || expected_username.len() > 128
        || vhost.len() > 128;
    if invalid {
        return Err(EngineError::InvalidRequest {
            detail: "RabbitMQ logical prune inputs are not exact, orphaned, and owned".to_owned(),
        });
    }

    Ok((credential.username(), vhost))
}

async fn listed(
    executor: &impl CommandExecutor,
    container: &crate::control_plane::engine::OwnedContainer,
    arguments: Vec<String>,
    expected: &str,
    action: &str,
    timeout: std::time::Duration,
) -> Result<bool, EngineError> {
    let command = command(arguments, action, timeout)?;
    let output = run_attached_command_capture(executor, container, &command).await?;
    let output = std::str::from_utf8(&output).map_err(|error| EngineError::Backend {
        detail: format!("{action} returned invalid UTF-8: {error}"),
    })?;
    Ok(output.lines().map(str::trim).any(|value| value == expected))
}

async fn run(
    executor: &impl CommandExecutor,
    container: &crate::control_plane::engine::OwnedContainer,
    arguments: Vec<String>,
    action: &str,
    timeout: std::time::Duration,
) -> Result<(), EngineError> {
    let command = command(arguments, action, timeout)?;
    run_attached_command(executor, container, &command).await
}

fn command(
    arguments: Vec<String>,
    action: &str,
    timeout: std::time::Duration,
) -> Result<AttachedCommandOptions, EngineError> {
    let request = CommandRequest::new(arguments, BTreeMap::new(), None)?;
    AttachedCommandOptions::new(request, Vec::new(), action, timeout)
}
