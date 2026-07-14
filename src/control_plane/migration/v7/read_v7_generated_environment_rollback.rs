use super::{V7GeneratedEnvironmentRollbackMaterial, V7InventoryError};
use crate::control_plane::retention::{
    BackupResourceIdentity, open_stored_backup_artifact, verify_stored_backup_artifact,
};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::Deserialize;

/// Verifies and decodes exact legacy `.env` bytes from private rollback state.
pub(crate) fn read_v7_generated_environment_rollback(
    material: &V7GeneratedEnvironmentRollbackMaterial,
    project_id: &str,
    evidence_revision: &str,
    verified_at_unix_seconds: i64,
    maximum_environment_bytes: usize,
) -> Result<Vec<u8>, V7InventoryError> {
    if project_id.is_empty() || verified_at_unix_seconds < 0 || maximum_environment_bytes == 0 {
        return Err(error("environment rollback read options are invalid"));
    }
    let maximum_rollback_bytes = maximum_environment_bytes
        .checked_mul(2)
        .and_then(|value| value.checked_add(4096))
        .ok_or_else(|| error("environment rollback byte bound exceeds supported limits"))?;
    if material.artifact_size_bytes()
        > u64::try_from(maximum_rollback_bytes)
            .map_err(|_| error("environment rollback byte bound exceeds filesystem limits"))?
    {
        return Err(error(
            "environment rollback exceeds the accepted generated-environment byte bound",
        ));
    }
    let stored = open_stored_backup_artifact(
        material
            .recovery_point()
            .to_str()
            .ok_or_else(|| error("environment rollback path is not valid UTF-8"))?,
    )
    .map_err(|source| error(format!("failed to open environment rollback: {source}")))?;
    let identity =
        BackupResourceIdentity::for_v7_generated_environment(project_id, evidence_revision);
    let verified = verify_stored_backup_artifact(&stored, verified_at_unix_seconds)
        .map_err(|source| error(format!("failed to verify environment rollback: {source}")))?;
    if !verified.matches_identity(&identity)
        || verified.artifact_sha256() != material.artifact_sha256()
        || verified.artifact_size_bytes() != material.artifact_size_bytes()
    {
        return Err(error(
            "environment rollback evidence does not match the accepted project inventory",
        ));
    }
    let envelope_bytes = std::fs::read(stored.artifact_file())
        .map_err(|source| error(format!("failed to read environment rollback: {source}")))?;
    let envelope = serde_json::from_slice::<EnvironmentRollbackEnvelope>(&envelope_bytes)
        .map_err(|source| error(format!("environment rollback is invalid: {source}")))?;
    let nonce = STANDARD
        .decode(&envelope.nonce_base64)
        .map_err(|_| error("environment rollback nonce is invalid"))?;
    let contents = STANDARD
        .decode(&envelope.contents_base64)
        .map_err(|_| error("environment rollback contents are invalid"))?;
    if envelope.schema_version != 1
        || envelope.project_id != project_id
        || envelope.evidence_revision != evidence_revision
        || nonce.len() != 32
        || u64::try_from(contents.len()).ok() != Some(envelope.source_size_bytes)
        || contents.len() > maximum_environment_bytes
        || envelope.source_modified_at_unix_seconds < 0
    {
        return Err(error(
            "environment rollback envelope does not match accepted source evidence",
        ));
    }

    Ok(contents)
}

fn error(detail: impl Into<String>) -> V7InventoryError {
    V7InventoryError::new(detail)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvironmentRollbackEnvelope {
    schema_version: u32,
    nonce_base64: String,
    project_id: String,
    evidence_revision: String,
    source_size_bytes: u64,
    source_modified_at_unix_seconds: i64,
    contents_base64: String,
}
