use super::{BackupArtifactManifest, BackupVerificationError, VerifiedBackupEvidence};
#[cfg(test)]
use sha2::{Digest, Sha256};

/// Verifies artifact bytes and returns unforgeable-in-domain deletion evidence.
#[cfg(test)]
pub(crate) fn verify_backup_artifact(
    manifest: &BackupArtifactManifest,
    artifact: &[u8],
    verified_at_unix_seconds: i64,
) -> Result<VerifiedBackupEvidence, BackupVerificationError> {
    let artifact_size_bytes =
        u64::try_from(artifact.len()).map_err(|_| BackupVerificationError::InvalidManifest {
            detail: "backup artifact size exceeds the supported range".to_owned(),
        })?;
    verify_backup_checksum(
        manifest,
        &hex::encode(Sha256::digest(artifact)),
        artifact_size_bytes,
        verified_at_unix_seconds,
    )
}

pub(super) fn verify_backup_checksum(
    manifest: &BackupArtifactManifest,
    artifact_sha256: &str,
    artifact_size_bytes: u64,
    verified_at_unix_seconds: i64,
) -> Result<VerifiedBackupEvidence, BackupVerificationError> {
    if verified_at_unix_seconds < 0 {
        return Err(BackupVerificationError::InvalidVerificationTime);
    }
    if verified_at_unix_seconds < manifest.created_at_unix_seconds() {
        return Err(BackupVerificationError::VerificationPredatesCreation);
    }

    if artifact_sha256 != manifest.artifact_sha256() {
        return Err(BackupVerificationError::ChecksumMismatch);
    }
    if artifact_size_bytes != manifest.artifact_size_bytes() {
        return Err(BackupVerificationError::SizeMismatch);
    }

    Ok(VerifiedBackupEvidence::new(
        manifest.resource_id().to_owned(),
        manifest.installation_id().to_owned(),
        manifest.resource_kind().to_owned(),
        manifest.compatibility_fingerprint().to_owned(),
        artifact_sha256.to_owned(),
        artifact_size_bytes,
        verified_at_unix_seconds,
    ))
}
