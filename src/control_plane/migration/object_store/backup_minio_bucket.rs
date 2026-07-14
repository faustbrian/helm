use super::MinioBackupOptions;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandExecutor, CommandRequest, OwnedContainer,
    StreamingCommandOptions, run_attached_command_capture, run_streaming_command,
};
use crate::control_plane::migration::{MigrationBackup, MigrationOperationError};
use crate::control_plane::retention::{
    BackupResourceIdentity, store_backup_artifact_from_async_reader, verify_stored_backup_artifact,
};
use crate::control_plane::state::{CredentialLifecycle, ResourceLifecycle};
use serde::Deserialize;
use std::collections::BTreeMap;
use tokio::io::{AsyncWriteExt, duplex};

const STREAM_BUFFER_BYTES: usize = 64 * 1024;
const VERSION_SCRIPT: &str = "set -eu\n\
    trap 'rm -rf \"$STACKCTL_MC_CONFIG\"' EXIT HUP INT TERM\n\
    mkdir -m 700 \"$STACKCTL_MC_CONFIG\"\n\
    mc --config-dir \"$STACKCTL_MC_CONFIG\" alias set stackctl \
    http://127.0.0.1:9000 \"$STACKCTL_ACCESS_KEY\" \
    \"$STACKCTL_SECRET_KEY\" >/dev/null\n\
    mc --config-dir \"$STACKCTL_MC_CONFIG\" version info \
    \"stackctl/$STACKCTL_BUCKET\" --json";
const EXPORT_SCRIPT: &str = "set -eu\n\
    trap 'rm -rf \"$STACKCTL_MC_CONFIG\" \"$STACKCTL_EXPORT_DIR\"' \
    EXIT HUP INT TERM\n\
    mkdir -m 700 \"$STACKCTL_MC_CONFIG\" \"$STACKCTL_EXPORT_DIR\"\n\
    mc --config-dir \"$STACKCTL_MC_CONFIG\" alias set stackctl \
    http://127.0.0.1:9000 \"$STACKCTL_ACCESS_KEY\" \
    \"$STACKCTL_SECRET_KEY\" >/dev/null\n\
    mc --config-dir \"$STACKCTL_MC_CONFIG\" mirror \
    \"stackctl/$STACKCTL_BUCKET\" \"$STACKCTL_EXPORT_DIR\" >/dev/null\n\
    tar -C \"$STACKCTL_EXPORT_DIR\" -cf - .";

/// Streams every current object from one unversioned project bucket.
pub(crate) async fn backup_minio_bucket(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &MinioBackupOptions<'_>,
) -> Result<MigrationBackup, MigrationOperationError> {
    let bucket = validate(container, options)?;
    prove_unversioned(executor, container, options, &bucket).await?;
    let prefix = format!(
        "/tmp/.stackctl-minio-{}-{}",
        bucket, options.created_at_unix_seconds
    );
    let environment = environment(
        options,
        &bucket,
        format!("{prefix}-export-config"),
        Some(format!("{prefix}-export-data")),
    );
    let request = CommandRequest::new(
        vec!["sh".to_owned(), "-c".to_owned(), EXPORT_SCRIPT.to_owned()],
        environment,
        None,
    )
    .map_err(|error| operation_error("MinIO backup request is invalid", error))?;
    let command = StreamingCommandOptions::new(request, "export MinIO bucket", options.timeout)
        .map_err(|error| operation_error("MinIO backup request is invalid", error))?;
    let identity =
        BackupResourceIdentity::from_logical(options.logical_resource, options.installation_id);
    let (mut backup_reader, mut command_output) = duplex(STREAM_BUFFER_BYTES);
    let mut command_input = tokio::io::empty();
    let export = async {
        let result = run_streaming_command(
            executor,
            container,
            &command,
            &mut command_input,
            &mut command_output,
        )
        .await;
        let close = command_output.shutdown().await;
        result.map_err(|error| operation_error("MinIO backup failed", error))?;
        close.map_err(|error| operation_error("MinIO backup output close failed", error))
    };
    let store = async {
        store_backup_artifact_from_async_reader(
            &identity,
            &mut backup_reader,
            options.created_at_unix_seconds,
            options.backup_root,
        )
        .await
        .map_err(|error| operation_error("MinIO backup storage failed", error))
    };
    let (_, stored) = futures_util::future::try_join(export, store).await?;
    let evidence = verify_stored_backup_artifact(&stored, options.created_at_unix_seconds)
        .map_err(|error| operation_error("MinIO backup verification failed", error))?;
    let reference = stored.recovery_point().to_str().ok_or_else(|| {
        MigrationOperationError::new("MinIO backup recovery point is not valid Unicode")
    })?;

    MigrationBackup::new(
        reference,
        evidence.artifact_sha256(),
        evidence.artifact_size_bytes(),
    )
}

async fn prove_unversioned(
    executor: &impl CommandExecutor,
    container: &OwnedContainer,
    options: &MinioBackupOptions<'_>,
    bucket: &str,
) -> Result<(), MigrationOperationError> {
    let config = format!(
        "/tmp/.stackctl-minio-{bucket}-{}-version-config",
        options.created_at_unix_seconds
    );
    let request = CommandRequest::new(
        vec!["sh".to_owned(), "-c".to_owned(), VERSION_SCRIPT.to_owned()],
        environment(options, bucket, config, None),
        None,
    )
    .map_err(|error| operation_error("MinIO version inventory request is invalid", error))?;
    let command = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "inspect MinIO bucket versioning",
        options.timeout,
    )
    .map_err(|error| operation_error("MinIO version inventory request is invalid", error))?;
    let output = run_attached_command_capture(executor, container, &command)
        .await
        .map_err(|error| operation_error("MinIO version inventory failed", error))?;
    validate_version_inventory(bucket, &output)
}

pub(super) fn validate_version_inventory(
    bucket: &str,
    output: &[u8],
) -> Result<(), MigrationOperationError> {
    let inventory = serde_json::from_slice::<VersionInventory>(output)
        .map_err(|error| operation_error("MinIO version inventory is malformed", error))?;
    if inventory.status != "success"
        || inventory
            .versioning
            .status
            .is_some_and(|value| !value.is_empty())
    {
        return Err(MigrationOperationError::new(format!(
            "MinIO bucket '{bucket}' uses versioning; latest-object export cannot preserve its history"
        )));
    }

    Ok(())
}

fn environment(
    options: &MinioBackupOptions<'_>,
    bucket: &str,
    config: String,
    export: Option<String>,
) -> BTreeMap<String, String> {
    let mut environment = BTreeMap::from([
        (
            "STACKCTL_ACCESS_KEY".to_owned(),
            options.credential.username().to_owned(),
        ),
        ("STACKCTL_BUCKET".to_owned(), bucket.to_owned()),
        ("STACKCTL_MC_CONFIG".to_owned(), config),
        (
            "STACKCTL_SECRET_KEY".to_owned(),
            options.credential.secret().to_owned(),
        ),
    ]);
    if let Some(export) = export {
        environment.insert("STACKCTL_EXPORT_DIR".to_owned(), export);
    }
    environment
}

fn validate(
    container: &OwnedContainer,
    options: &MinioBackupOptions<'_>,
) -> Result<String, MigrationOperationError> {
    let logical = options.logical_resource;
    let credential = options.credential;
    let identity = format!("{}-{}", logical.project_id(), logical.service_id());
    let bucket = format!("stackctl-{identity}");
    let username = format!("st_{}", identity.replace('-', "_"));
    let invalid = options.installation_id.is_empty()
        || options.created_at_unix_seconds < 0
        || !options.backup_root.is_absolute()
        || options.backup_root.to_str().is_none()
        || options.timeout.is_zero()
        || logical.kind() != "minio_bucket_policy"
        || logical.lifecycle() != ResourceLifecycle::Active
        || logical.logical_resource_id() != credential.credential_id()
        || credential.project_id() != Some(logical.project_id())
        || credential.service_id() != logical.service_id()
        || credential.username() != username
        || credential.secret().is_empty()
        || credential.lifecycle() != CredentialLifecycle::Active
        || bucket.len() > 63
        || container.metadata().installation_id() != options.installation_id
        || container.metadata().compatibility_fingerprint() != logical.compatibility_fingerprint();
    if invalid {
        return Err(MigrationOperationError::new(
            "MinIO backup request does not match an active owned logical resource",
        ));
    }

    Ok(bucket)
}

#[derive(Deserialize)]
struct VersionInventory {
    status: String,
    #[serde(default)]
    versioning: Versioning,
}

#[derive(Default, Deserialize)]
struct Versioning {
    status: Option<String>,
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
