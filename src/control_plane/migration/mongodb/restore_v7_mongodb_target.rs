use super::mongodb_connection_uri::mongodb_connection_uri;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, OwnedContainer,
    StreamingCommandOptions, run_attached_command, run_streaming_command,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::retention::StoredBackupArtifact;
use crate::control_plane::shared_infrastructure::MongoDbLogicalResourcePlan;
use crate::control_plane::state::{CredentialLifecycle, CredentialRecord};
use std::collections::BTreeMap;
use std::time::Duration;

const RESTORE_SCRIPT: &str = "set -eu\nexec mongorestore --uri=\"$STACKCTL_MONGODB_URI\" --archive --drop --nsInclude=\"$STACKCTL_MONGODB_SOURCE.*\" --nsFrom=\"$STACKCTL_MONGODB_SOURCE.*\" --nsTo=\"$STACKCTL_MONGODB_TARGET.*\"";

/// Recreates and restores a deterministic v8 database so preparation is replay-safe.
pub(super) async fn restore_v7_mongodb_target(
    executor: &(impl CommandExecutor + Sync),
    container: &OwnedContainer,
    source_database_name: &str,
    plan: &MongoDbLogicalResourcePlan,
    administrator: &CredentialRecord,
    target_credential: &CredentialRecord,
    stored: &StoredBackupArtifact,
    timeout: Duration,
) -> Result<(), MigrationOperationError> {
    reset_and_provision_target(executor, container, plan, administrator, timeout).await?;
    let request = restore_request(source_database_name, plan, administrator)?;
    let command =
        StreamingCommandOptions::new(request, "restore v7 data into v8 MongoDB target", timeout)
            .map_err(|error| operation_error("v8 MongoDB restore request is invalid", error))?;
    let mut artifact = tokio::fs::File::open(stored.artifact_file())
        .await
        .map_err(|error| operation_error("open v7 MongoDB restore artifact", error))?;
    let mut output = tokio::io::sink();
    run_streaming_command(executor, container, &command, &mut artifact, &mut output)
        .await
        .map_err(|error| operation_error("restore v8 MongoDB migration target", error))?;

    verify_target_credential(target_credential, plan)
}

fn restore_request(
    source_database_name: &str,
    plan: &MongoDbLogicalResourcePlan,
    administrator: &CredentialRecord,
) -> Result<CommandRequest, MigrationOperationError> {
    CommandRequest::new(
        vec!["sh".to_owned(), "-c".to_owned(), RESTORE_SCRIPT.to_owned()],
        BTreeMap::from([
            (
                "STACKCTL_MONGODB_URI".to_owned(),
                mongodb_connection_uri(
                    administrator.username(),
                    administrator.secret(),
                    "admin",
                    "admin",
                ),
            ),
            (
                "STACKCTL_MONGODB_SOURCE".to_owned(),
                source_database_name.to_owned(),
            ),
            (
                "STACKCTL_MONGODB_TARGET".to_owned(),
                plan.database_name().to_owned(),
            ),
        ]),
        None,
    )
    .map_err(|error| operation_error("v8 MongoDB restore request is invalid", error))
}

async fn reset_and_provision_target(
    executor: &(impl CommandExecutor + Sync),
    container: &OwnedContainer,
    plan: &MongoDbLogicalResourcePlan,
    administrator: &CredentialRecord,
    timeout: Duration,
) -> Result<(), MigrationOperationError> {
    let expected = format!(
        "migration/{}/mongodb-bootstrap",
        container.metadata().resource_id().unwrap_or_default()
    );
    if administrator.lifecycle() != CredentialLifecycle::Active
        || administrator.credential_id() != expected
        || administrator.username() != "stackctl_admin"
        || administrator.secret().is_empty()
    {
        return Err(MigrationOperationError::new(
            "v8 MongoDB target administrator is not the active migration bootstrap user",
        ));
    }
    let script = format!(
        "const admin = connect({});\nif (!admin.auth({}, {})) {{ quit(1); }}\nadmin.getSiblingDB({}).dropDatabase();\n",
        json("mongodb://127.0.0.1:27017/admin")?,
        json(administrator.username())?,
        json(administrator.secret())?,
        json(plan.database_name())?,
    );
    let request = CommandRequest::new(
        vec![
            "mongosh".to_owned(),
            "--quiet".to_owned(),
            "--nodb".to_owned(),
        ],
        BTreeMap::new(),
        None,
    )
    .map_err(|error| operation_error("v8 MongoDB target reset is invalid", error))?;
    let command = AttachedCommandOptions::new(
        request,
        script.into_bytes(),
        "reset v8 MongoDB migration target",
        timeout,
    )
    .map_err(|error| operation_error("v8 MongoDB target reset is invalid", error))?;
    run_attached_command(executor, container, &command)
        .await
        .map_err(|error| operation_error("reset v8 MongoDB migration target", error))?;

    let request = CommandRequest::new(
        plan.command_arguments()
            .into_iter()
            .map(str::to_owned)
            .collect(),
        BTreeMap::new(),
        None,
    )
    .map_err(|error| operation_error("v8 MongoDB target provisioning is invalid", error))?;
    let command = AttachedCommandOptions::new(
        request,
        plan.stdin_script().as_bytes().to_vec(),
        "provision v8 MongoDB migration target",
        timeout,
    )
    .map_err(|error| operation_error("v8 MongoDB target provisioning is invalid", error))?;
    run_attached_command(executor, container, &command)
        .await
        .map_err(|error| operation_error("provision v8 MongoDB migration target", error))
}

fn verify_target_credential(
    credential: &CredentialRecord,
    plan: &MongoDbLogicalResourcePlan,
) -> Result<(), MigrationOperationError> {
    if credential.lifecycle() != CredentialLifecycle::Active
        || credential.username() != plan.username()
    {
        return Err(MigrationOperationError::new(
            "v8 MongoDB target credential differs from the deterministic plan",
        ));
    }
    Ok(())
}

fn json(value: &str) -> Result<String, MigrationOperationError> {
    serde_json::to_string(value)
        .map_err(|error| operation_error("encode v8 MongoDB reset input", error))
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
