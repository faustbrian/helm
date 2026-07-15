use super::{MinioRestoreOptions, backup_minio_bucket::export_relative_path};
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, ContainerVolumeArchive,
    OwnedContainer, OwnedVolume, StreamingCommandOptions, run_attached_command_capture,
    run_streaming_command,
};
use crate::control_plane::migration::MigrationOperationError;
use crate::control_plane::retention::{
    BackupResourceIdentity, open_stored_backup_artifact, verify_stored_backup_artifact,
};
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use std::collections::BTreeMap;

const RESTORE_SCRIPT: &str = "set -eu\n\
    trap 'rm -rf \"$STACKCTL_MC_CONFIG\"' EXIT HUP INT TERM\n\
    mkdir -m 700 \"$STACKCTL_MC_CONFIG\"\n\
    mc --config-dir \"$STACKCTL_MC_CONFIG\" alias set stackctl \
    http://127.0.0.1:9000 \"$STACKCTL_ACCESS_KEY\" \
    \"$STACKCTL_SECRET_KEY\" >/dev/null\n\
    mc --config-dir \"$STACKCTL_MC_CONFIG\" mirror --overwrite --remove \
    \"$STACKCTL_RESTORE_DIR\" \"stackctl/$STACKCTL_BUCKET\" >/dev/null";
const PREPARE_SCRIPT: &str = "set -eu\n\
    rm -rf \"$STACKCTL_RESTORE_DIR\"\n\
    mkdir -p \"$STACKCTL_RESTORE_PARENT\"\n\
    chmod 700 \"$STACKCTL_RESTORE_PARENT\"";
const CLEANUP_SCRIPT: &str = "set -eu\nrm -rf \"$STACKCTL_RESTORE_DIR\"";

/// Replaces one exact unversioned bucket from an immutable verified archive.
pub(crate) async fn restore_minio_bucket(
    executor: &(impl CommandExecutor + ContainerVolumeArchive),
    container: &OwnedContainer,
    volume: &OwnedVolume,
    options: &MinioRestoreOptions<'_>,
) -> Result<(), MigrationOperationError> {
    validate(container, volume, options)?;
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

    let restore_relative = export_relative_path(
        options.target_bucket_name,
        recovery.created_at_unix_seconds(),
    );
    let restore_absolute = std::path::Path::new("/data").join(&restore_relative);
    let restore_parent = restore_absolute
        .parent()
        .ok_or_else(|| MigrationOperationError::new("MinIO restore path has no parent"))?;
    let restore_environment = BTreeMap::from([
        (
            "STACKCTL_RESTORE_DIR".to_owned(),
            restore_absolute.to_string_lossy().into_owned(),
        ),
        (
            "STACKCTL_RESTORE_PARENT".to_owned(),
            restore_parent.to_string_lossy().into_owned(),
        ),
    ]);
    let restored = async {
        run_script(
            executor,
            container,
            PREPARE_SCRIPT,
            restore_environment.clone(),
            "prepare MinIO restore staging",
            options.timeout,
        )
        .await?;
        let artifact = tokio::fs::File::open(stored.artifact_file())
            .await
            .map_err(|error| operation_error("MinIO restore artifact open failed", error))?;
        executor
            .upload_volume_subpath_archive(container, volume, &restore_relative, Box::new(artifact))
            .await
            .map_err(|error| operation_error("MinIO restore archive staging failed", error))?;
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
                (
                    "STACKCTL_RESTORE_DIR".to_owned(),
                    restore_absolute.to_string_lossy().into_owned(),
                ),
                (
                    "STACKCTL_SECRET_KEY".to_owned(),
                    options.credential.secret().to_owned(),
                ),
            ]),
            None,
        )
        .map_err(|error| operation_error("MinIO restore request is invalid", error))?;
        let command =
            StreamingCommandOptions::new(request, "restore MinIO bucket", options.timeout)
                .map_err(|error| operation_error("MinIO restore request is invalid", error))?;
        let mut artifact = tokio::io::empty();
        let mut output = tokio::io::sink();

        run_streaming_command(executor, container, &command, &mut artifact, &mut output)
            .await
            .map_err(|error| operation_error("MinIO restore failed", error))
    }
    .await;
    let cleanup = run_script(
        executor,
        container,
        CLEANUP_SCRIPT,
        restore_environment,
        "clean up MinIO restore staging",
        options.timeout,
    )
    .await;
    restored?;
    cleanup
}

async fn run_script(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    script: &str,
    environment: BTreeMap<String, String>,
    description: &str,
    timeout: std::time::Duration,
) -> Result<(), MigrationOperationError> {
    let request = CommandRequest::new(
        vec!["sh".to_owned(), "-c".to_owned(), script.to_owned()],
        environment,
        None,
    )
    .map_err(|error| operation_error("MinIO staging request is invalid", error))?;
    let command = AttachedCommandOptions::new(request, Vec::new(), description, timeout)
        .map_err(|error| operation_error("MinIO staging request is invalid", error))?;
    run_attached_command_capture(executor, container, &command)
        .await
        .map(|_| ())
        .map_err(|error| operation_error(description, error))
}

fn validate(
    container: &OwnedContainer,
    volume: &OwnedVolume,
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
        || volume.name() != logical.shared_resource_id()
        || volume.metadata().installation_id() != options.installation_id
        || volume.metadata().compatibility_fingerprint() != recovery.compatibility_fingerprint()
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
