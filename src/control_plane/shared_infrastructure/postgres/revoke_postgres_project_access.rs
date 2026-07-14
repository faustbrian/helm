use super::{
    POSTGRES_BOOTSTRAP_USERNAME, PostgresAccessRevocationOptions, PostgresLogicalResourcePlan,
};
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError,
    run_attached_command_capture,
};
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use std::collections::BTreeMap;

/// Disables one exact tenant role while retaining its database and data.
pub(crate) async fn revoke_postgres_project_access(
    executor: &impl CommandExecutor,
    options: PostgresAccessRevocationOptions<'_>,
) -> Result<bool, EngineError> {
    let plan = validate(&options)?;
    let request = CommandRequest::new(
        vec![
            "psql".to_owned(),
            "--no-psqlrc".to_owned(),
            "--quiet".to_owned(),
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
    let sql = format!(
        "SELECT CASE\n\
             WHEN EXISTS (\n\
                 SELECT 1 FROM pg_roles\n\
                 WHERE rolname = '{}' AND rolcanlogin\n\
             ) THEN 'true'\n\
             ELSE 'false'\n\
         END AS stackctl_changed \\gset\n\
         \\if :stackctl_changed\n\
         ALTER ROLE {} NOLOGIN;\n\
         \\endif\n\
         \\echo :stackctl_changed\n",
        plan.role_name(),
        plan.role_name(),
    );
    let command = AttachedCommandOptions::new(
        request,
        sql.into_bytes(),
        "revoke orphaned PostgreSQL tenant access".to_owned(),
        options.timeout,
    )?;
    let output = run_attached_command_capture(executor, options.container, &command).await?;

    boolean_reply(&output)
}

fn validate(
    options: &PostgresAccessRevocationOptions<'_>,
) -> Result<PostgresLogicalResourcePlan, EngineError> {
    let logical = options.logical_resource;
    let credential = options.credential;
    let administrator = options.administrator;
    let fingerprint = logical
        .compatibility_fingerprint()
        .strip_prefix("sha256:")
        .unwrap_or_default();
    let expected_administrator = format!("shared/{fingerprint}/postgresql-bootstrap");
    let plan = PostgresLogicalResourcePlan::new(
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
        || logical.kind() != "postgres_database_and_role"
        || logical.lifecycle() != ResourceLifecycle::Orphaned
        || logical.orphaned_at_unix_seconds().is_none()
        || logical.logical_resource_id() != plan.database_name()
        || !plan.matches_credential(credential)
        || credential.lifecycle() != CredentialLifecycle::Disabled
        || administrator.credential_id() != expected_administrator
        || administrator.project_id().is_some()
        || administrator.service_id() != "postgresql"
        || administrator.username() != POSTGRES_BOOTSTRAP_USERNAME
        || administrator.secret().is_empty()
        || administrator.lifecycle() != CredentialLifecycle::Active;
    if invalid {
        return Err(EngineError::InvalidRequest {
            detail: "PostgreSQL access revocation inputs are not exact, orphaned, and owned"
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
            detail: "PostgreSQL role revocation returned a malformed boolean".to_owned(),
        }),
        Err(error) => Err(EngineError::Backend {
            detail: format!("PostgreSQL role revocation returned invalid UTF-8: {error}"),
        }),
    }
}
