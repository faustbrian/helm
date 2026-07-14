use super::V7MinioCredential;
use crate::control_plane::engine::{
    AttachedCommandOptions, CommandRequest, V7ContainerCommandExecutor, V7ContainerCommandTarget,
    run_v7_attached_command_capture,
};
use crate::control_plane::migration::MigrationOperationError;
use std::collections::BTreeMap;
use std::time::Duration;

const VERIFY_SCRIPT: &str = "set -eu\n\
    trap 'rm -rf \"$STACKCTL_MC_CONFIG\"' EXIT HUP INT TERM\n\
    rm -rf \"$STACKCTL_MC_CONFIG\"\n\
    mkdir -m 700 \"$STACKCTL_MC_CONFIG\"\n\
    mc --config-dir \"$STACKCTL_MC_CONFIG\" alias set stackctl \
    http://127.0.0.1:9000 \"$STACKCTL_ACCESS_KEY\" \
    \"$STACKCTL_SECRET_KEY\" >/dev/null\n\
    mc --config-dir \"$STACKCTL_MC_CONFIG\" stat \
    \"stackctl/$STACKCTL_BUCKET\" >/dev/null\n\
    printf '%s\\n' \"$STACKCTL_BUCKET\"";

pub(super) async fn verify_v7_minio_source(
    executor: &(impl V7ContainerCommandExecutor + Sync),
    target: &V7ContainerCommandTarget,
    credential: &V7MinioCredential,
    bucket: &str,
    timeout: Duration,
) -> Result<(), MigrationOperationError> {
    let request = verification_request(credential.access_key(), credential.secret_key(), bucket)?;
    let command = AttachedCommandOptions::new(
        request,
        Vec::new(),
        "verify retained v7 MinIO bucket",
        timeout,
    )
    .map_err(|error| operation_error("v7 MinIO verification is invalid", error))?;
    let output = run_v7_attached_command_capture(executor, target, &command)
        .await
        .map_err(|error| operation_error("verify retained v7 MinIO bucket", error))?;
    verify_output(bucket, &output)
}

pub(super) fn verification_request(
    access_key: &str,
    secret_key: &str,
    bucket: &str,
) -> Result<CommandRequest, MigrationOperationError> {
    CommandRequest::new(
        vec!["sh".to_owned(), "-c".to_owned(), VERIFY_SCRIPT.to_owned()],
        BTreeMap::from([
            ("STACKCTL_ACCESS_KEY".to_owned(), access_key.to_owned()),
            ("STACKCTL_BUCKET".to_owned(), bucket.to_owned()),
            (
                "STACKCTL_MC_CONFIG".to_owned(),
                format!("/tmp/.stackctl-v7-minio-{bucket}-verify"),
            ),
            ("STACKCTL_SECRET_KEY".to_owned(), secret_key.to_owned()),
        ]),
        None,
    )
    .map_err(|error| operation_error("MinIO verification request is invalid", error))
}

pub(super) fn verify_output(bucket: &str, output: &[u8]) -> Result<(), MigrationOperationError> {
    let expected = format!("{bucket}\n");
    if output != expected.as_bytes() {
        return Err(MigrationOperationError::new(
            "MinIO endpoint returned unexpected bucket identity evidence",
        ));
    }
    Ok(())
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}
