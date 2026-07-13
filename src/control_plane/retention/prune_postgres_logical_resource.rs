use super::PostgresLogicalPruneOptions;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, run_attached_command,
};
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use std::collections::BTreeMap;

const POSTGRES_BOOTSTRAP_USERNAME: &str = "stackctl_admin";

/// Idempotently deletes one exact orphaned database and role inside a shared instance.
pub(crate) async fn prune_postgres_logical_resource(
    executor: &impl CommandExecutor,
    options: PostgresLogicalPruneOptions<'_>,
) -> Result<(), EngineError> {
    validate(&options)?;
    let request = CommandRequest::new(
        vec![
            "psql".to_owned(),
            "--no-psqlrc".to_owned(),
            "--set=ON_ERROR_STOP=1".to_owned(),
            format!("--username={POSTGRES_BOOTSTRAP_USERNAME}"),
            "--dbname=postgres".to_owned(),
        ],
        BTreeMap::from([(
            "PGPASSWORD".to_owned(),
            options.administrator.secret().to_owned(),
        )]),
        None,
    )?;
    let command = AttachedCommandOptions::new(
        request,
        deletion_sql(
            options.logical_resource.logical_resource_id(),
            options.credential.username(),
        )
        .into_bytes(),
        "prune confirmed PostgreSQL logical resource",
        options.timeout,
    )?;

    run_attached_command(executor, options.container, &command).await
}

fn validate(options: &PostgresLogicalPruneOptions<'_>) -> Result<(), EngineError> {
    let logical = options.logical_resource;
    let credential = options.credential;
    let administrator = options.administrator;
    let invalid = options.installation_id.is_empty()
        || options.timeout.is_zero()
        || options.container.metadata().installation_id() != options.installation_id
        || logical.kind() != "postgres_database_and_role"
        || logical.lifecycle() == ResourceLifecycle::Active
        || logical.orphaned_at_unix_seconds().is_none()
        || !valid_identifier(logical.logical_resource_id())
        || credential.project_id() != Some(logical.project_id())
        || credential.service_id() != logical.service_id()
        || credential.lifecycle() != CredentialLifecycle::Disabled
        || !valid_identifier(credential.username())
        || administrator.project_id().is_some()
        || administrator.username() != POSTGRES_BOOTSTRAP_USERNAME
        || administrator.secret().is_empty()
        || administrator.lifecycle() != CredentialLifecycle::Active;
    if invalid {
        return Err(EngineError::InvalidRequest {
            detail: "PostgreSQL logical prune inputs are not exact, orphaned, and owned".to_owned(),
        });
    }

    Ok(())
}

fn deletion_sql(database_name: &str, role_name: &str) -> String {
    format!(
        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity \
         WHERE datname = '{database_name}' AND pid <> pg_backend_pid();\n\
         DROP DATABASE IF EXISTS {database_name};\n\
         DROP ROLE IF EXISTS {role_name};\n"
    )
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && value.bytes().enumerate().all(|(index, byte)| match byte {
            b'a'..=b'z' | b'_' => true,
            b'0'..=b'9' => index > 0,
            _ => false,
        })
}
