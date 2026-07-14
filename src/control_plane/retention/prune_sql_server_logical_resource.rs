use super::SqlServerLogicalPruneOptions;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, run_attached_command,
};
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use std::collections::BTreeMap;

const SQLCMD_PATH: &str = "/opt/mssql-tools18/bin/sqlcmd";

/// Idempotently deletes one exact orphaned SQL Server database and login.
pub(crate) async fn prune_sql_server_logical_resource(
    executor: &impl CommandExecutor,
    options: SqlServerLogicalPruneOptions<'_>,
) -> Result<(), EngineError> {
    validate(&options)?;
    let request = CommandRequest::new(
        vec![
            SQLCMD_PATH.to_owned(),
            "-b".to_owned(),
            "-C".to_owned(),
            "-S".to_owned(),
            "127.0.0.1".to_owned(),
            "-U".to_owned(),
            "sa".to_owned(),
            "-d".to_owned(),
            "master".to_owned(),
        ],
        BTreeMap::from([(
            "SQLCMDPASSWORD".to_owned(),
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
        "prune confirmed SQL Server logical resource",
        options.timeout,
    )?;

    run_attached_command(executor, options.container, &command).await
}

fn validate(options: &SqlServerLogicalPruneOptions<'_>) -> Result<(), EngineError> {
    let logical = options.logical_resource;
    let credential = options.credential;
    let administrator = options.administrator;
    let invalid = options.installation_id.is_empty()
        || options.timeout.is_zero()
        || options.container.metadata().installation_id() != options.installation_id
        || options.container.metadata().compatibility_fingerprint()
            != logical.compatibility_fingerprint()
        || logical.kind() != "sqlserver_database"
        || logical.lifecycle() == ResourceLifecycle::Active
        || logical.orphaned_at_unix_seconds().is_none()
        || !valid_identifier(logical.logical_resource_id())
        || credential.project_id() != Some(logical.project_id())
        || credential.service_id() != logical.service_id()
        || credential.lifecycle() != CredentialLifecycle::Disabled
        || !valid_identifier(credential.username())
        || administrator.project_id().is_some()
        || administrator.service_id() != "sqlserver"
        || administrator.username() != "sa"
        || administrator.secret().is_empty()
        || administrator.lifecycle() != CredentialLifecycle::Active;
    if invalid {
        return Err(EngineError::InvalidRequest {
            detail: "SQL Server logical prune inputs are not exact, orphaned, and owned".to_owned(),
        });
    }

    Ok(())
}

fn deletion_sql(database: &str, login: &str) -> String {
    format!(
        "IF DB_ID(N'{database}') IS NOT NULL\n\
         BEGIN\n\
             ALTER DATABASE [{database}] SET SINGLE_USER WITH ROLLBACK IMMEDIATE;\n\
             DROP DATABASE [{database}];\n\
         END;\n\
         IF SUSER_ID(N'{login}') IS NOT NULL DROP LOGIN [{login}];\n"
    )
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().enumerate().all(|(index, byte)| match byte {
            b'a'..=b'z' | b'_' => true,
            b'0'..=b'9' => index > 0,
            _ => false,
        })
}
