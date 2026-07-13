use super::verify_backup_artifact::verify_backup_checksum;
use super::{
    BackupArtifactManifest, BackupVerificationError, StoredBackupArtifact, VerifiedBackupEvidence,
};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Read;

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
    let mut artifact =
        File::open(stored.artifact_file()).map_err(|error| BackupVerificationError::Storage {
            detail: format!(
                "failed to open backup artifact '{}': {error}",
                stored.artifact_file().display()
            ),
        })?;
    let mut digest = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count =
            artifact
                .read(&mut buffer)
                .map_err(|error| BackupVerificationError::Storage {
                    detail: format!(
                        "failed to read backup artifact '{}': {error}",
                        stored.artifact_file().display()
                    ),
                })?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
        size = size.saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
    }

    verify_backup_checksum(
        &manifest,
        &hex::encode(digest.finalize()),
        size,
        verified_at_unix_seconds,
    )
}
