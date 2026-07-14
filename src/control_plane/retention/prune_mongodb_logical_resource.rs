use super::MongoDbLogicalPruneOptions;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, EngineError, run_attached_command,
};
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use std::collections::BTreeMap;

const MONGODB_BOOTSTRAP_USERNAME: &str = "stackctl_admin";

/// Idempotently deletes one exact orphaned database and user from MongoDB.
pub(crate) async fn prune_mongodb_logical_resource(
    executor: &impl CommandExecutor,
    options: MongoDbLogicalPruneOptions<'_>,
) -> Result<(), EngineError> {
    validate(&options)?;
    let request = CommandRequest::new(
        vec![
            "mongosh".to_owned(),
            "--quiet".to_owned(),
            "--nodb".to_owned(),
        ],
        BTreeMap::new(),
        None,
    )?;
    let command = AttachedCommandOptions::new(
        request,
        deletion_script(&options)?.into_bytes(),
        "prune confirmed MongoDB logical resource",
        options.timeout,
    )?;

    run_attached_command(executor, options.container, &command).await
}

fn validate(options: &MongoDbLogicalPruneOptions<'_>) -> Result<(), EngineError> {
    let logical = options.logical_resource;
    let credential = options.credential;
    let administrator = options.administrator;
    let invalid = options.installation_id.is_empty()
        || options.timeout.is_zero()
        || options.container.metadata().installation_id() != options.installation_id
        || options.container.metadata().compatibility_fingerprint()
            != logical.compatibility_fingerprint()
        || logical.kind() != "mongodb_database"
        || logical.lifecycle() == ResourceLifecycle::Active
        || logical.orphaned_at_unix_seconds().is_none()
        || !valid_identifier(logical.logical_resource_id())
        || credential.project_id() != Some(logical.project_id())
        || credential.service_id() != logical.service_id()
        || credential.lifecycle() != CredentialLifecycle::Disabled
        || !valid_identifier(credential.username())
        || administrator.project_id().is_some()
        || administrator.service_id() != "mongodb"
        || administrator.username() != MONGODB_BOOTSTRAP_USERNAME
        || administrator.secret().is_empty()
        || administrator.lifecycle() != CredentialLifecycle::Active;
    if invalid {
        return Err(EngineError::InvalidRequest {
            detail: "MongoDB logical prune inputs are not exact, orphaned, and owned".to_owned(),
        });
    }

    Ok(())
}

fn deletion_script(options: &MongoDbLogicalPruneOptions<'_>) -> Result<String, EngineError> {
    let database = json_string(options.logical_resource.logical_resource_id())?;
    let username = json_string(options.credential.username())?;
    let administrator = json_string(options.administrator.username())?;
    let secret = json_string(options.administrator.secret())?;

    Ok(format!(
        "try {{\n\
         const admin = connect(\"mongodb://127.0.0.1:27017/admin\");\n\
         if (!admin.auth({administrator}, {secret})) {{ throw new Error(\"admin authentication failed\"); }}\n\
         const target = admin.getSiblingDB({database});\n\
         if (target.getUser({username}) !== null) {{ target.dropUser({username}); }}\n\
         target.dropDatabase();\n\
         }} catch (error) {{ print(error); quit(1); }}\n"
    ))
}

fn json_string(value: &str) -> Result<String, EngineError> {
    serde_json::to_string(value).map_err(|error| EngineError::InvalidRequest {
        detail: format!("MongoDB logical prune value encoding failed: {error}"),
    })
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
