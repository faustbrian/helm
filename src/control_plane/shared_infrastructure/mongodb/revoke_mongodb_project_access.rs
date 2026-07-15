use super::{MongoDbAccessRevocationOptions, MongoDbLogicalResourcePlan};
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError,
    run_attached_command_capture,
};
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use std::collections::BTreeMap;

const ADMINISTRATOR_PASSWORD_KEY: &str = "STACKCTL_MONGODB_ADMIN_PASSWORD";

/// Removes one exact database user while retaining its database and collections.
pub(crate) async fn revoke_mongodb_project_access(
    executor: &impl CommandExecutor,
    options: MongoDbAccessRevocationOptions<'_>,
) -> Result<bool, EngineError> {
    let plan = validate(&options)?;
    let database = json_string(plan.database_name())?;
    let username = json_string(plan.username())?;
    let request = CommandRequest::new(
        vec![
            "mongosh".to_owned(),
            "--quiet".to_owned(),
            "--nodb".to_owned(),
            "--file".to_owned(),
            "/dev/stdin".to_owned(),
        ],
        BTreeMap::from([(
            ADMINISTRATOR_PASSWORD_KEY.to_owned(),
            options.administrator.secret().to_owned(),
        )]),
        None,
    )?;
    let script = format!(
        "try {{\n\
         const admin = connect(\"mongodb://127.0.0.1:27017/admin\");\n\
         const password = process.env.{ADMINISTRATOR_PASSWORD_KEY};\n\
         if (!admin.auth(\"stackctl_admin\", password)) {{\n\
           throw new Error(\"admin authentication failed\");\n\
         }}\n\
         const target = admin.getSiblingDB({database});\n\
         const changed = target.getUser({username}) !== null;\n\
         if (changed) {{ target.dropUser({username}); }}\n\
         print(changed ? \"true\" : \"false\");\n\
         }} catch (error) {{ print(error); quit(1); }}\n"
    );
    let command = AttachedCommandOptions::new(
        request,
        script.into_bytes(),
        "revoke orphaned MongoDB tenant access".to_owned(),
        options.timeout,
    )?;
    let output = run_attached_command_capture(executor, options.container, &command).await?;

    boolean_reply(&output)
}

fn validate(
    options: &MongoDbAccessRevocationOptions<'_>,
) -> Result<MongoDbLogicalResourcePlan, EngineError> {
    let logical = options.logical_resource;
    let credential = options.credential;
    let administrator = options.administrator;
    let fingerprint = logical
        .compatibility_fingerprint()
        .strip_prefix("sha256:")
        .unwrap_or_default();
    let expected_administrator = format!("shared/{fingerprint}/mongodb-bootstrap");
    let plan = MongoDbLogicalResourcePlan::new(
        logical.project_id(),
        logical.service_id(),
        CredentialSecret::new(credential.secret().to_owned()),
        CredentialSecret::new(administrator.secret().to_owned()),
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
        || logical.kind() != "mongodb_database"
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
        || administrator.service_id() != "mongodb"
        || administrator.username() != "stackctl_admin"
        || administrator.secret().is_empty()
        || administrator.lifecycle() != CredentialLifecycle::Active;
    if invalid {
        return Err(EngineError::InvalidRequest {
            detail: "MongoDB access revocation inputs are not exact, orphaned, and owned"
                .to_owned(),
        });
    }

    Ok(plan)
}

fn json_string(value: &str) -> Result<String, EngineError> {
    serde_json::to_string(value).map_err(|error| EngineError::InvalidRequest {
        detail: format!("failed to encode MongoDB access revocation input: {error}"),
    })
}

fn boolean_reply(output: &[u8]) -> Result<bool, EngineError> {
    match std::str::from_utf8(output).map(str::trim) {
        Ok("true") => Ok(true),
        Ok("false") => Ok(false),
        Ok(_) => Err(EngineError::Backend {
            detail: "MongoDB user revocation returned a malformed boolean".to_owned(),
        }),
        Err(error) => Err(EngineError::Backend {
            detail: format!("MongoDB user revocation returned invalid UTF-8: {error}"),
        }),
    }
}
