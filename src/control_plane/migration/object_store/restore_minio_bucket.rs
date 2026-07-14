use super::MinioRestoreOptions;
use crate::control_plane::engine::{
    CommandExecutor, CommandRequest, OwnedContainer, StreamingCommandOptions, run_streaming_command,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::retention::{
    BackupResourceIdentity, open_stored_backup_artifact, verify_stored_backup_artifact,
};
use crate::control_plane::state::{CredentialLifecycle, MigrationPhase, ResourceLifecycle};
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
    let checkpoint = options.checkpoint;
    let reference = required(
        checkpoint.backup_reference(),
        "MinIO restore checkpoint has no backup reference",
    )?;
    let expected_checksum = required(
        checkpoint.backup_artifact_sha256(),
        "MinIO restore checkpoint has no backup checksum",
    )?;
    let expected_size = checkpoint.backup_artifact_size_bytes().ok_or_else(|| {
        MigrationOperationError::new("MinIO restore checkpoint has no backup size")
    })?;
    let identity = BackupResourceIdentity::from_logical(
        options.source_logical_resource,
        options.installation_id,
    );
    let stored = open_stored_backup_artifact(reference)
        .map_err(|error| operation_error("MinIO restore backup is unavailable", error))?;
    let evidence = verify_stored_backup_artifact(&stored, options.verified_at_unix_seconds)
        .map_err(|error| operation_error("MinIO restore backup verification failed", error))?;
    if !evidence.matches_identity(&identity)
        || evidence.artifact_sha256() != expected_checksum
        || evidence.artifact_size_bytes() != expected_size
    {
        return Err(MigrationOperationError::new(
            "MinIO restore backup does not match its durable checkpoint",
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
    let checkpoint = options.checkpoint;
    let logical = options.source_logical_resource;
    let credential = options.credential;
    let identity = format!("{}-{}", logical.project_id(), logical.service_id());
    let expected_bucket = format!("stackctl-{identity}");
    let expected_username = format!("st_{}", identity.replace('-', "_"));
    let invalid = checkpoint.phase() != MigrationPhase::TargetProvisioned
        || options.installation_id.is_empty()
        || options.target_bucket_name != expected_bucket
        || options.target_bucket_name.len() > 63
        || checkpoint.target_resource_id() != Some(options.target_bucket_name)
        || logical.kind() != "minio_bucket_policy"
        || logical.project_id() != checkpoint.project_id()
        || logical.lifecycle() != ResourceLifecycle::Active
        || logical.compatibility_fingerprint() != checkpoint.source_compatibility_fingerprint()
        || logical.logical_resource_id() != credential.credential_id()
        || credential.project_id() != Some(checkpoint.project_id())
        || credential.service_id() != logical.service_id()
        || credential.username() != expected_username
        || credential.secret().is_empty()
        || credential.lifecycle() != CredentialLifecycle::Active
        || options.timeout.is_zero()
        || options.verified_at_unix_seconds < checkpoint.updated_at_unix_seconds()
        || container.metadata().installation_id() != options.installation_id
        || container.metadata().compatibility_fingerprint()
            != checkpoint.target_compatibility_fingerprint();
    if invalid {
        return Err(MigrationOperationError::new(
            "MinIO restore request does not match its owned migration target",
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
