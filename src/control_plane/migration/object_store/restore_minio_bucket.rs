use super::MinioRestoreOptions;
use crate::control_plane::engine::{
    CommandExecutor, CommandRequest, OwnedContainer, StreamingCommandOptions, run_streaming_command,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::retention::{
    BackupResourceIdentity, StoredBackupArtifact, open_stored_backup_artifact,
    verify_stored_backup_artifact,
};
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use std::collections::BTreeMap;

const RESTORE_SCRIPT: &str = "set -eu\n\
    trap 'rm -rf \"$STACKCTL_MC_CONFIG\" \"$STACKCTL_RESTORE_DIR\"' \
    EXIT HUP INT TERM\n\
    mkdir -m 700 \"$STACKCTL_MC_CONFIG\" \"$STACKCTL_RESTORE_DIR\"\n\
    tar -C \"$STACKCTL_RESTORE_DIR\" -xf -\n\
    mc --config-dir \"$STACKCTL_MC_CONFIG\" alias set stackctl \
    http://127.0.0.1:9000 \"$STACKCTL_ACCESS_KEY\" \
    \"$STACKCTL_SECRET_KEY\" >/dev/null\n\
    mc --config-dir \"$STACKCTL_MC_CONFIG\" mirror --overwrite --remove \
    \"$STACKCTL_RESTORE_DIR\" \"stackctl/$STACKCTL_BUCKET\" >/dev/null";

/// Replaces one exact unversioned bucket from an immutable verified archive.
pub(crate) async fn restore_minio_bucket(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &MinioRestoreOptions<'_>,
) -> Result<(), MigrationOperationError> {
    validate(container, options)?;
    let recovery = options.recovery_point;
    let reference = recovery.reference();
    let expected_checksum = recovery.artifact_sha256();
    let expected_size = recovery.artifact_size_bytes();
    let identity =
        BackupResourceIdentity::from_logical(options.logical_resource, options.installation_id);
    let stored = open_stored_backup_artifact(reference)
        .map_err(|error| operation_error("MinIO restore backup is unavailable", error))?;
    let evidence = verify_stored_backup_artifact(&stored, options.verified_at_unix_seconds)
        .map_err(|error| operation_error("MinIO restore backup verification failed", error))?;
    if !evidence.matches_identity(&identity)
        || evidence.artifact_sha256() != expected_checksum
        || evidence.artifact_size_bytes() != expected_size
    {
        return Err(MigrationOperationError::new(
            "MinIO restore backup does not match its recovery point",
        ));
    }

    restore_verified_minio_bucket(executor, container, options, &stored).await
}

/// Restores an already identity-verified archive into one exact target bucket.
pub(super) async fn restore_verified_minio_bucket(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &MinioRestoreOptions<'_>,
    stored: &StoredBackupArtifact,
) -> Result<(), MigrationOperationError> {
    validate(container, options)?;
    if stored.recovery_point().to_str() != Some(options.recovery_point.reference()) {
        return Err(MigrationOperationError::new(
            "MinIO verified restore path differs from its recovery point",
        ));
    }
    let evidence = verify_stored_backup_artifact(stored, options.verified_at_unix_seconds)
        .map_err(|error| operation_error("MinIO restore backup verification failed", error))?;
    if evidence.artifact_sha256() != options.recovery_point.artifact_sha256()
        || evidence.artifact_size_bytes() != options.recovery_point.artifact_size_bytes()
    {
        return Err(MigrationOperationError::new(
            "MinIO restore artifact changed after verification",
        ));
    }

    let prefix = format!(
        "/tmp/.stackctl-minio-{}-restore-{}",
        options.target_bucket_name, options.verified_at_unix_seconds
    );
    let request = CommandRequest::new(
        vec!["sh".to_owned(), "-c".to_owned(), RESTORE_SCRIPT.to_owned()],
        BTreeMap::from([
            (
                "STACKCTL_ACCESS_KEY".to_owned(),
                options.credential.username().to_owned(),
            ),
            (
                "STACKCTL_BUCKET".to_owned(),
                options.target_bucket_name.to_owned(),
            ),
            ("STACKCTL_MC_CONFIG".to_owned(), format!("{prefix}-config")),
            ("STACKCTL_RESTORE_DIR".to_owned(), format!("{prefix}-data")),
            (
                "STACKCTL_SECRET_KEY".to_owned(),
                options.credential.secret().to_owned(),
            ),
        ]),
        None,
    )
    .map_err(|error| operation_error("MinIO restore request is invalid", error))?;
    let command = StreamingCommandOptions::new(request, "restore MinIO bucket", options.timeout)
        .map_err(|error| operation_error("MinIO restore request is invalid", error))?;
    let mut artifact = tokio::fs::File::open(stored.artifact_file())
        .await
        .map_err(|error| operation_error("MinIO restore artifact open failed", error))?;
    let mut output = tokio::io::sink();

    run_streaming_command(executor, container, &command, &mut artifact, &mut output)
        .await
        .map_err(|error| operation_error("MinIO restore failed", error))
}

fn validate(
    container: &OwnedContainer,
    options: &MinioRestoreOptions<'_>,
) -> Result<(), MigrationOperationError> {
    let recovery = options.recovery_point;
    let logical = options.logical_resource;
    let credential = options.credential;
    let identity = format!("{}-{}", logical.project_id(), logical.service_id());
    let expected_bucket = format!("stackctl-{identity}");
    let expected_username = format!("st_{}", identity.replace('-', "_"));
    let exact_recovery = recovery.project_id() == logical.project_id()
        && recovery.service_id() == logical.service_id()
        && recovery.logical_resource_id() == logical.logical_resource_id()
        && recovery.resource_kind() == logical.kind()
        && recovery.compatibility_fingerprint() == logical.compatibility_fingerprint();
    let invalid = options.installation_id.is_empty()
        || options.target_bucket_name != expected_bucket
        || options.target_bucket_name.len() > 63
        || logical.kind() != "minio_bucket_policy"
        || logical.lifecycle() != ResourceLifecycle::Active
        || logical.logical_resource_id() != credential.credential_id()
        || credential.project_id() != Some(logical.project_id())
        || credential.service_id() != logical.service_id()
        || credential.username() != expected_username
        || credential.secret().is_empty()
        || credential.lifecycle() != CredentialLifecycle::Active
        || options.timeout.is_zero()
        || options.verified_at_unix_seconds < recovery.verified_at_unix_seconds()
        || container.metadata().installation_id() != options.installation_id
        || container.metadata().compatibility_fingerprint() != recovery.compatibility_fingerprint()
        || !exact_recovery;
    if invalid {
        return Err(MigrationOperationError::new(
            "MinIO restore request does not match its owned recovery point",
        ));
    }

    Ok(())
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
