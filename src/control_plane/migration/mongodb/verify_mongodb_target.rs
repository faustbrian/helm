use super::{MongoDbVerifyTargetOptions, mongodb_connection_uri::mongodb_connection_uri};
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, ResourceKind, RetentionClass,
    run_attached_command_capture,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::state::{CredentialLifecycle, MigrationPhase};
use std::collections::BTreeMap;

const VERIFY_SCRIPT: &str = "set -eu\nexec mongosh \"$STACKCTL_MONGODB_URI\" --quiet \
    --eval 'print(db.getName()); print(db.runCommand({ ping: 1 }).ok);'";

/// Requires tenant-authenticated access to the exact restored database.
pub(crate) async fn verify_mongodb_target(
    executor: &impl CommandExecutor,
    container: &crate::control_plane::engine::OwnedContainer,
    options: &MongoDbVerifyTargetOptions<'_>,
) -> Result<(), MigrationOperationError> {
    validate(container, options)?;
    let request = CommandRequest::new(
        vec!["sh".to_owned(), "-c".to_owned(), VERIFY_SCRIPT.to_owned()],
        BTreeMap::from([
            (
                "STACKCTL_MONGODB_URI".to_owned(),
                mongodb_connection_uri(
                    options.credential.username(),
                    options.credential.secret(),
                    options.target_database_name,
                    options.target_database_name,
                ),
            ),
            (
                "STACKCTL_MONGODB_DATABASE".to_owned(),
                options.target_database_name.to_owned(),
            ),
        ]),
        None,
    )
    .map_err(|error| operation_error("MongoDB target verification request is invalid", error))?;
    let command = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "verify MongoDB target catalog",
        options.timeout,
    )
    .map_err(|error| operation_error("MongoDB target verification request is invalid", error))?;
    let output = run_attached_command_capture(executor, container, &command)
        .await
        .map_err(|error| operation_error("MongoDB target verification failed", error))?;
    let expected = format!("{}\n1\n", options.target_database_name);
    if output != expected.as_bytes() {
        return Err(MigrationOperationError::new(
            "MongoDB target verification returned unexpected evidence",
        ));
    }

    Ok(())
}

fn validate(
    container: &crate::control_plane::engine::OwnedContainer,
    options: &MongoDbVerifyTargetOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let checkpoint = options.checkpoint;
    let metadata = container.metadata();
    let invalid = checkpoint.phase() != MigrationPhase::DataRestored
        || checkpoint.target_resource_id() != Some(options.target_database_name)
        || options.installation_id.is_empty()
        || options.target_database_name.is_empty()
        || options.credential.project_id() != Some(checkpoint.project_id())
        || options.credential.service_id().is_empty()
        || options.credential.username().is_empty()
        || options.credential.secret().is_empty()
        || options.credential.lifecycle() != CredentialLifecycle::Active
        || options.timeout.is_zero()
        || metadata.installation_id() != options.installation_id
        || metadata.kind() != ResourceKind::ProjectService
        || metadata.project_id() != Some(checkpoint.project_id())
        || metadata.resource_id() != Some(checkpoint.migration_id())
        || metadata.retention() != RetentionClass::Persistent
        || metadata.compatibility_fingerprint() != checkpoint.target_compatibility_fingerprint();
    if invalid {
        return Err(MigrationOperationError::new(
            "MongoDB target verification does not match its durable checkpoint",
        ));
    }

    Ok(())
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
