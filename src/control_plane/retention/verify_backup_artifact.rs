use super::{BackupArtifactManifest, BackupVerificationError, VerifiedBackupEvidence};
use sha2::{Digest, Sha256};

/// Verifies artifact bytes and returns unforgeable-in-domain deletion evidence.
pub(crate) fn verify_backup_artifact(
    manifest: &BackupArtifactManifest,
    artifact: &[u8],
    verified_at_unix_seconds: i64,
) -> Result<VerifiedBackupEvidence, BackupVerificationError> {
    if verified_at_unix_seconds < 0 {
        return Err(BackupVerificationError::InvalidVerificationTime);
    }
    if verified_at_unix_seconds < manifest.created_at_unix_seconds() {
        return Err(BackupVerificationError::VerificationPredatesCreation);
    }

    let artifact_sha256 = hex::encode(Sha256::digest(artifact));
    if artifact_sha256 != manifest.artifact_sha256() {
        return Err(BackupVerificationError::ChecksumMismatch);
    }

    Ok(VerifiedBackupEvidence::new(
        manifest.resource_id().to_owned(),
        manifest.installation_id().to_owned(),
        manifest.resource_kind().to_owned(),
        manifest.compatibility_fingerprint().to_owned(),
        artifact_sha256,
        verified_at_unix_seconds,
    ))
}
