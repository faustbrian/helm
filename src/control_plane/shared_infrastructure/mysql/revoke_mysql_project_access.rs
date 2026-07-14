use super::{MySqlAccessRevocationOptions, MySqlLogicalResourcePlan};
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError,
    run_attached_command_capture,
};
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use std::collections::BTreeMap;

/// Removes one exact tenant user while retaining its schema and data.
pub(crate) async fn revoke_mysql_project_access(
    executor: &impl CommandExecutor,
    options: MySqlAccessRevocationOptions<'_>,
) -> Result<bool, EngineError> {
    let plan = validate(&options)?;
    let request = CommandRequest::new(
        vec![
            options.flavor.client_executable().to_owned(),
            "--batch".to_owned(),
            "--skip-column-names".to_owned(),
            "--user=root".to_owned(),
        ],
        BTreeMap::from([(
            "MYSQL_PWD".to_owned(),
            options.administrator.secret().to_owned(),
        )]),
        None,
    )?;
    let sql = format!(
        "SELECT EXISTS(\n\
             SELECT 1 FROM mysql.user\n\
             WHERE User = '{}' AND Host = '%'\n\
         );\n\
         DROP USER IF EXISTS '{}'@'%';\n",
        plan.username(),
        plan.username(),
    );
    let command = AttachedCommandOptions::new(
        request,
        sql.into_bytes(),
        format!(
            "revoke orphaned {} tenant access",
            options.flavor.implementation()
        ),
        options.timeout,
    )?;
    let output = run_attached_command_capture(executor, options.container, &command).await?;

    boolean_integer_reply(&output)
}

fn validate(
    options: &MySqlAccessRevocationOptions<'_>,
) -> Result<MySqlLogicalResourcePlan, EngineError> {
    let logical = options.logical_resource;
    let credential = options.credential;
    let administrator = options.administrator;
    let implementation = options.flavor.implementation();
    let fingerprint = logical
        .compatibility_fingerprint()
        .strip_prefix("sha256:")
        .unwrap_or_default();
    let expected_administrator = format!("shared/{fingerprint}/{implementation}-bootstrap");
    let plan = MySqlLogicalResourcePlan::new(
        options.flavor,
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
        || logical.kind() != format!("{implementation}_database")
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
        || administrator.service_id() != implementation
        || administrator.username() != "root"
        || administrator.secret().is_empty()
        || administrator.lifecycle() != CredentialLifecycle::Active;
    if invalid {
        return Err(EngineError::InvalidRequest {
            detail: "MySQL-family access revocation inputs are not exact, orphaned, and owned"
                .to_owned(),
        });
    }

    Ok(plan)
}

fn boolean_integer_reply(output: &[u8]) -> Result<bool, EngineError> {
    match std::str::from_utf8(output).map(str::trim) {
        Ok("1") => Ok(true),
        Ok("0") => Ok(false),
        Ok(_) => Err(EngineError::Backend {
            detail: "MySQL-family user revocation returned a malformed boolean".to_owned(),
        }),
        Err(error) => Err(EngineError::Backend {
            detail: format!("MySQL-family user revocation returned invalid UTF-8: {error}"),
        }),
    }
}
