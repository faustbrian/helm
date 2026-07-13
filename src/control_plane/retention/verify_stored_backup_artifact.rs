use super::{
    BackupArtifactManifest, BackupVerificationError, StoredBackupArtifact, VerifiedBackupEvidence,
    verify_backup_artifact,
};
use std::fs;

/// Rereads a stored artifact and verifies its manifest before deletion use.
pub(crate) fn verify_stored_backup_artifact(
    stored: &StoredBackupArtifact,
    verified_at_unix_seconds: i64,
) -> Result<VerifiedBackupEvidence, BackupVerificationError> {
    let manifest_bytes =
        fs::read(stored.manifest_file()).map_err(|error| BackupVerificationError::Storage {
            detail: format!(
                "failed to read backup manifest '{}': {error}",
                stored.manifest_file().display()
            ),
        })?;
    let manifest: BackupArtifactManifest =
        serde_json::from_slice(&manifest_bytes).map_err(|error| {
            BackupVerificationError::InvalidManifest {
                detail: format!(
                    "backup manifest '{}' is invalid: {error}",
                    stored.manifest_file().display()
                ),
            }
        })?;
    manifest.validate()?;
    let artifact =
        fs::read(stored.artifact_file()).map_err(|error| BackupVerificationError::Storage {
            detail: format!(
                "failed to read backup artifact '{}': {error}",
                stored.artifact_file().display()
            ),
        })?;

    verify_backup_artifact(&manifest, &artifact, verified_at_unix_seconds)
}
