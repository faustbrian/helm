use super::{V7MinioCredential, backup_minio_bucket::validate_version_inventory};
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandRequest, StreamingCommandOptions, V7ContainerCommandExecutor,
    V7ContainerCommandTarget, run_v7_attached_command_capture, run_v7_streaming_command,
};
use crate::control_plane::migration::{MigrationBackup, MigrationOperationError};
use crate::control_plane::retention::{
    BackupResourceIdentity, store_backup_artifact_from_async_reader, verify_stored_backup_artifact,
};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;
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

/// Streams every current object from one exact accepted unversioned bucket.
pub(super) async fn backup_v7_minio_bucket(
    executor: &(impl V7ContainerCommandExecutor + Sync),
    target: &V7ContainerCommandTarget,
    credential: &V7MinioCredential,
    bucket: &str,
    identity: &BackupResourceIdentity,
    backup_root: &Path,
    created_at_unix_seconds: i64,
    verified_at_unix_seconds: i64,
    timeout: Duration,
) -> Result<MigrationBackup, MigrationOperationError> {
    prove_unversioned(
        executor,
        target,
        credential,
        bucket,
        created_at_unix_seconds,
        timeout,
    )
    .await?;
    let prefix = format!("/tmp/.stackctl-v7-minio-{bucket}-{created_at_unix_seconds}");
    let request = CommandRequest::new(
        vec!["sh".to_owned(), "-c".to_owned(), EXPORT_SCRIPT.to_owned()],
        environment(
            credential,
            bucket,
            format!("{prefix}-export-config"),
            Some(format!("{prefix}-export-data")),
        ),
        None,
    )
    .map_err(|error| operation_error("v7 MinIO backup request is invalid", error))?;
    let command = StreamingCommandOptions::new(request, "export accepted v7 MinIO bucket", timeout)
        .map_err(|error| operation_error("v7 MinIO backup request is invalid", error))?;
    let (mut reader, mut output) = duplex(STREAM_BUFFER_BYTES);
    let mut input = tokio::io::empty();
    let export = async {
        let result =
            run_v7_streaming_command(executor, target, &command, &mut input, &mut output).await;
        let close = output.shutdown().await;
        result.map_err(|error| operation_error("v7 MinIO backup failed", error))?;
        close.map_err(|error| operation_error("v7 MinIO backup output close failed", error))
    };
    let store = async {
        store_backup_artifact_from_async_reader(
            identity,
            &mut reader,
            created_at_unix_seconds,
            backup_root,
        )
        .await
        .map_err(|error| operation_error("v7 MinIO backup storage failed", error))
    };
    let (_, stored) = futures_util::future::try_join(export, store).await?;
    let evidence = verify_stored_backup_artifact(&stored, verified_at_unix_seconds)
        .map_err(|error| operation_error("v7 MinIO backup verification failed", error))?;
    let reference = stored.recovery_point().to_str().ok_or_else(|| {
        MigrationOperationError::new("v7 MinIO recovery path is not valid Unicode")
    })?;
    MigrationBackup::new(
        reference,
        evidence.artifact_sha256(),
        evidence.artifact_size_bytes(),
    )
}

async fn prove_unversioned(
    executor: &(impl V7ContainerCommandExecutor + Sync),
    target: &V7ContainerCommandTarget,
    credential: &V7MinioCredential,
    bucket: &str,
    created_at_unix_seconds: i64,
    timeout: Duration,
) -> Result<(), MigrationOperationError> {
    let request = CommandRequest::new(
        vec!["sh".to_owned(), "-c".to_owned(), VERSION_SCRIPT.to_owned()],
        environment(
            credential,
            bucket,
            format!("/tmp/.stackctl-v7-minio-{bucket}-{created_at_unix_seconds}-version"),
            None,
        ),
        None,
    )
    .map_err(|error| operation_error("v7 MinIO version request is invalid", error))?;
    let command = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "inspect accepted v7 MinIO bucket versioning",
        timeout,
    )
    .map_err(|error| operation_error("v7 MinIO version request is invalid", error))?;
    let output = run_v7_attached_command_capture(executor, target, &command)
        .await
        .map_err(|error| operation_error("v7 MinIO version inventory failed", error))?;
    validate_version_inventory(bucket, &output)
}

fn environment(
    credential: &V7MinioCredential,
    bucket: &str,
    config: String,
    export: Option<String>,
) -> BTreeMap<String, String> {
    let mut environment = BTreeMap::from([
        (
            "STACKCTL_ACCESS_KEY".to_owned(),
            credential.access_key().to_owned(),
        ),
        ("STACKCTL_BUCKET".to_owned(), bucket.to_owned()),
        ("STACKCTL_MC_CONFIG".to_owned(), config),
        (
            "STACKCTL_SECRET_KEY".to_owned(),
            credential.secret_key().to_owned(),
        ),
    ]);
    if let Some(export) = export {
        environment.insert("STACKCTL_EXPORT_DIR".to_owned(), export);
    }
    environment
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
