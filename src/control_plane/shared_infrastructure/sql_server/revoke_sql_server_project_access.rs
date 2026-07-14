use super::{SqlServerAccessRevocationOptions, SqlServerLogicalResourcePlan};
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError,
    run_attached_command_capture,
};
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use std::collections::BTreeMap;

const SQLCMD_PATH: &str = "/opt/mssql-tools18/bin/sqlcmd";

/// Disables one exact tenant login while retaining its database and mapped user.
pub(crate) async fn revoke_sql_server_project_access(
    executor: &impl CommandExecutor,
    options: SqlServerAccessRevocationOptions<'_>,
) -> Result<bool, EngineError> {
    let plan = validate(&options)?;
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
            "-h".to_owned(),
            "-1".to_owned(),
            "-W".to_owned(),
        ],
        BTreeMap::from([(
            "SQLCMDPASSWORD".to_owned(),
            options.administrator.secret().to_owned(),
        )]),
        None,
    )?;
    let sql = format!(
        "SET NOCOUNT ON;\n\
         DECLARE @changed bit = CASE WHEN EXISTS (\n\
             SELECT 1 FROM sys.server_principals\n\
             WHERE name = N'{}' AND is_disabled = 0\n\
         ) THEN 1 ELSE 0 END;\n\
         IF @changed = 1\n\
             ALTER LOGIN [{}] DISABLE;\n\
         SELECT CASE WHEN @changed = 1 THEN N'true' ELSE N'false' END;\n",
        plan.username(),
        plan.username(),
    );
    let command = AttachedCommandOptions::new(
        request,
        sql.into_bytes(),
        "revoke orphaned SQL Server tenant access".to_owned(),
        options.timeout,
    )?;
    let output = run_attached_command_capture(executor, options.container, &command).await?;

    boolean_reply(&output)
}

fn validate(
    options: &SqlServerAccessRevocationOptions<'_>,
) -> Result<SqlServerLogicalResourcePlan, EngineError> {
    let logical = options.logical_resource;
    let credential = options.credential;
    let administrator = options.administrator;
    let fingerprint = logical
        .compatibility_fingerprint()
        .strip_prefix("sha256:")
        .unwrap_or_default();
    let expected_administrator = format!("shared/{fingerprint}/sqlserver-bootstrap");
    let plan = SqlServerLogicalResourcePlan::new(
        logical.project_id(),
        logical.service_id(),
        CredentialSecret::new(credential.secret().to_owned()),
    )
    .map_err(|error| EngineError::InvalidRequest {
        detail: error.to_string(),
    })?;
    let invalid = options.installation_id.is_empty()
        || options.timeout.is_zero()
        || fingerprint.len() != 64
        || options.container.metadata().installation_id() != options.installation_id
        || options.container.metadata().compatibility_fingerprint()
            != logical.compatibility_fingerprint()
        || logical.kind() != "sqlserver_database"
        || logical.lifecycle() != ResourceLifecycle::Orphaned
        || logical.orphaned_at_unix_seconds().is_none()
        || logical.logical_resource_id() != plan.credential_id()
        || credential.credential_id() != plan.credential_id()
        || credential.project_id() != Some(logical.project_id())
        || credential.service_id() != logical.service_id()
        || credential.username() != plan.username()
        || credential.lifecycle() != CredentialLifecycle::Disabled
        || administrator.credential_id() != expected_administrator
        || administrator.project_id().is_some()
        || administrator.service_id() != "sqlserver"
        || administrator.username() != "sa"
        || administrator.secret().is_empty()
        || administrator.lifecycle() != CredentialLifecycle::Active;
    if invalid {
        return Err(EngineError::InvalidRequest {
            detail: "SQL Server access revocation inputs are not exact, orphaned, and owned"
                .to_owned(),
        });
    }

    Ok(plan)
}

fn boolean_reply(output: &[u8]) -> Result<bool, EngineError> {
    match std::str::from_utf8(output).map(str::trim) {
        Ok("true") => Ok(true),
        Ok("false") => Ok(false),
        Ok(_) => Err(EngineError::Backend {
            detail: "SQL Server login revocation returned a malformed boolean".to_owned(),
        }),
        Err(error) => Err(EngineError::Backend {
            detail: format!("SQL Server login revocation returned invalid UTF-8: {error}"),
        }),
    }
}
