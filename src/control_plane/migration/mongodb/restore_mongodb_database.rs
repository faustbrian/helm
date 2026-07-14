use super::{MongoDbRestoreOptions, mongodb_connection_uri::mongodb_connection_uri};
use crate::control_plane::engine::{
    CommandExecutor, CommandRequest, ResourceKind, RetentionClass, StreamingCommandOptions,
    run_streaming_command,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::retention::{
    BackupResourceIdentity, open_stored_backup_artifact, verify_stored_backup_artifact,
};
use crate::control_plane::state::{CredentialLifecycle, MigrationPhase, ResourceLifecycle};
use std::collections::BTreeMap;

const RESTORE_SCRIPT: &str = "set -eu\nexec mongorestore --uri=\"$STACKCTL_MONGODB_URI\" \
    --archive --drop --nsInclude=\"$STACKCTL_MONGODB_DATABASE.*\"";

/// Restores one exact journaled archive into an isolated MongoDB target.
pub(crate) async fn restore_mongodb_database(
    executor: &impl CommandExecutor,
    container: &crate::control_plane::engine::OwnedContainer,
    options: &MongoDbRestoreOptions<'_>,
) -> Result<(), MigrationOperationError> {
    validate(container, options)?;
    let checkpoint = options.checkpoint;
    let reference = required(
        checkpoint.backup_reference(),
        "MongoDB restore checkpoint has no backup reference",
    )?;
    let expected_checksum = required(
        checkpoint.backup_artifact_sha256(),
        "MongoDB restore checkpoint has no backup checksum",
    )?;
    let expected_size = checkpoint.backup_artifact_size_bytes().ok_or_else(|| {
        MigrationOperationError::new("MongoDB restore checkpoint has no backup size")
    })?;
    let identity = BackupResourceIdentity::from_logical(
        options.source_logical_resource,
        options.installation_id,
    );
    let stored = open_stored_backup_artifact(reference)
        .map_err(|error| operation_error("MongoDB restore backup is unavailable", error))?;
    let evidence = verify_stored_backup_artifact(&stored, options.verified_at_unix_seconds)
        .map_err(|error| operation_error("MongoDB restore backup verification failed", error))?;
    if !evidence.matches_identity(&identity)
        || evidence.artifact_sha256() != expected_checksum
        || evidence.artifact_size_bytes() != expected_size
    {
        return Err(MigrationOperationError::new(
            "MongoDB restore backup does not match its durable checkpoint",
        ));
    }

    let request = CommandRequest::new(
        vec!["sh".to_owned(), "-c".to_owned(), RESTORE_SCRIPT.to_owned()],
        BTreeMap::from([
            (
                "STACKCTL_MONGODB_URI".to_owned(),
                mongodb_connection_uri(
                    options.administrator.username(),
                    options.administrator.secret(),
                    options.target_database_name,
                    "admin",
                ),
            ),
            (
                "STACKCTL_MONGODB_DATABASE".to_owned(),
                options.target_database_name.to_owned(),
            ),
        ]),
        None,
    )
    .map_err(|error| operation_error("MongoDB restore request is invalid", error))?;
    let command =
        StreamingCommandOptions::new(request, "restore MongoDB database", options.timeout)
            .map_err(|error| operation_error("MongoDB restore request is invalid", error))?;
    let mut artifact = tokio::fs::File::open(stored.artifact_file())
        .await
        .map_err(|error| operation_error("MongoDB restore artifact open failed", error))?;
    let mut output = tokio::io::sink();

    run_streaming_command(executor, container, &command, &mut artifact, &mut output)
        .await
        .map_err(|error| operation_error("MongoDB restore failed", error))
}

fn validate(
    container: &crate::control_plane::engine::OwnedContainer,
    options: &MongoDbRestoreOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let checkpoint = options.checkpoint;
    let expected_credential_id =
        format!("migration/{}/mongodb-bootstrap", checkpoint.migration_id());
    let metadata = container.metadata();
    let invalid = checkpoint.phase() != MigrationPhase::TargetProvisioned
        || options.installation_id.is_empty()
        || options.target_database_name.is_empty()
        || checkpoint.target_resource_id() != Some(options.target_database_name)
        || options.source_logical_resource.kind() != "mongodb_database"
        || options.source_logical_resource.project_id() != checkpoint.project_id()
        || options.source_logical_resource.lifecycle() != ResourceLifecycle::Active
        || options.source_logical_resource.compatibility_fingerprint()
            != checkpoint.source_compatibility_fingerprint()
        || options.administrator.credential_id() != expected_credential_id
        || options.administrator.project_id() != Some(checkpoint.project_id())
        || options.administrator.service_id() != "mongodb"
        || options.administrator.username().is_empty()
        || options.administrator.secret().is_empty()
        || options.administrator.lifecycle() != CredentialLifecycle::Active
        || options.timeout.is_zero()
        || options.verified_at_unix_seconds < checkpoint.updated_at_unix_seconds()
        || metadata.installation_id() != options.installation_id
        || metadata.kind() != ResourceKind::ProjectService
        || metadata.project_id() != Some(checkpoint.project_id())
        || metadata.resource_id() != Some(checkpoint.migration_id())
        || metadata.retention() != RetentionClass::Persistent
        || metadata.compatibility_fingerprint() != checkpoint.target_compatibility_fingerprint();
    if invalid {
        return Err(MigrationOperationError::new(
            "MongoDB restore request does not match its owned migration target",
        ));
    }

    Ok(())
}

fn required<'value>(
    value: Option<&'value str>,
    detail: &str,
) -> Result<&'value str, MigrationOperationError> {
    value.ok_or_else(|| MigrationOperationError::new(detail))
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
