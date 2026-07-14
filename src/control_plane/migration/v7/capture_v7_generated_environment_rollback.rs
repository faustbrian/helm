use super::inventory_v7_host_artifacts::{environment_keys, read_regular};
use super::{
    V7GeneratedEnvironmentRollbackMaterial, V7GeneratedEnvironmentRollbackOptions, V7InventoryError,
};
use crate::control_plane::retention::{
    BackupResourceIdentity, store_backup_artifact_for_identity, verify_stored_backup_artifact,
};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::Serialize;
use std::time::UNIX_EPOCH;

/// Copies stable exact `.env` bytes into a randomized private rollback envelope.
pub(crate) fn capture_v7_generated_environment_rollback(
    options: V7GeneratedEnvironmentRollbackOptions<'_>,
) -> Result<V7GeneratedEnvironmentRollbackMaterial, V7InventoryError> {
    validate_options(&options)?;
    let artifact = read_regular(
        options.expected.path(),
        options.maximum_environment_bytes,
        true,
    )?
    .ok_or_else(|| error("legacy generated environment disappeared during rollback capture"))?;
    let modified_at_unix_seconds = i64::try_from(
        artifact
            .modified_at
            .duration_since(UNIX_EPOCH)
            .map_err(|_| error("legacy generated environment predates the Unix epoch"))?
            .as_secs(),
    )
    .map_err(|_| error("legacy generated environment has an unsupported modification time"))?;
    let contents = std::str::from_utf8(&artifact.bytes)
        .map_err(|_| error("legacy generated environment is not valid UTF-8"))?;
    if artifact.size_bytes != options.expected.size_bytes()
        || modified_at_unix_seconds != options.expected.modified_at_unix_seconds()
        || environment_keys(contents) != options.expected.keys()
    {
        return Err(error(
            "legacy generated environment changed after inventory; plan migration again",
        ));
    }
    let mut nonce = [0_u8; 32];
    getrandom::getrandom(&mut nonce)
        .map_err(|source| error(format!("failed to randomize rollback material: {source}")))?;
    let envelope = serde_json::to_vec(&EnvironmentRollbackEnvelope {
        schema_version: 1,
        nonce_base64: STANDARD.encode(nonce),
        project_id: options.project_id,
        evidence_revision: options.evidence_revision,
        source_size_bytes: artifact.size_bytes,
        source_modified_at_unix_seconds: modified_at_unix_seconds,
        contents_base64: STANDARD.encode(&artifact.bytes),
    })
    .map_err(|source| error(format!("failed to encode environment rollback: {source}")))?;
    let identity = BackupResourceIdentity::for_v7_generated_environment(
        options.project_id,
        options.evidence_revision,
    );
    let stored = store_backup_artifact_for_identity(
        &identity,
        &envelope,
        options.created_at_unix_seconds,
        options.backup_root,
    )
    .map_err(|source| error(format!("failed to store environment rollback: {source}")))?;
    let verified = verify_stored_backup_artifact(&stored, options.created_at_unix_seconds)
        .map_err(|source| error(format!("failed to verify environment rollback: {source}")))?;
    if !verified.matches_identity(&identity) {
        return Err(error(
            "stored environment rollback does not match accepted project evidence",
        ));
    }

    Ok(V7GeneratedEnvironmentRollbackMaterial::new(
        stored.recovery_point().to_path_buf(),
        verified.artifact_sha256().to_owned(),
        verified.artifact_size_bytes(),
    ))
}

fn validate_options(
    options: &V7GeneratedEnvironmentRollbackOptions<'_>,
) -> Result<(), V7InventoryError> {
    let valid_revision = options.evidence_revision.len() == 64
        && options
            .evidence_revision
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if options.project_id.is_empty()
        || !valid_revision
        || !options.backup_root.is_absolute()
        || options.maximum_environment_bytes == 0
        || options.created_at_unix_seconds < 0
    {
        return Err(error(
            "environment rollback capture requires project identity, accepted evidence, an absolute backup root, a positive byte bound, and non-negative time",
        ));
    }

    Ok(())
}

fn error(detail: impl Into<String>) -> V7InventoryError {
    V7InventoryError::new(detail)
}

#[derive(Serialize)]
struct EnvironmentRollbackEnvelope<'value> {
    schema_version: u32,
    nonce_base64: String,
    project_id: &'value str,
    evidence_revision: &'value str,
    source_size_bytes: u64,
    source_modified_at_unix_seconds: i64,
    contents_base64: String,
}
