use super::hashing_reader::HashingReader;
use super::{
    BackupVerificationError, RestoreError, RestoreTarget, StoredBackupArtifact,
    VerifiedBackupEvidence, verify_stored_backup_artifact,
};
use crate::control_plane::state::ResourceRecord;
use std::fs::File;

/// Restores immutable bytes through isolated staging and atomic target cutover.
pub(crate) fn restore_verified_backup(
    restore_id: &str,
    resource: &ResourceRecord,
    stored: &StoredBackupArtifact,
    verified_at_unix_seconds: i64,
    target: &mut dyn RestoreTarget,
) -> Result<VerifiedBackupEvidence, RestoreError> {
    if restore_id.is_empty()
        || matches!(restore_id, "." | "..")
        || restore_id.len() > 128
        || !restore_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(RestoreError::InvalidRestoreId);
    }

    let evidence = verify_stored_backup_artifact(stored, verified_at_unix_seconds)?;
    if !evidence.matches(resource) {
        return Err(RestoreError::BackupResourceMismatch);
    }

    let artifact =
        File::open(stored.artifact_file()).map_err(|error| BackupVerificationError::Storage {
            detail: format!(
                "failed to reopen backup artifact '{}': {error}",
                stored.artifact_file().display()
            ),
        })?;
    let mut input = HashingReader::new(artifact);
    if let Err(error) = target.stage(restore_id, resource, &mut input) {
        return rollback_after(RestoreError::Target(error), restore_id, resource, target);
    }

    let (artifact_sha256, artifact_size_bytes, reached_eof) = input.finish();
    if !reached_eof {
        return rollback_after(
            RestoreError::IncompleteArtifact,
            restore_id,
            resource,
            target,
        );
    }
    if artifact_sha256 != evidence.artifact_sha256() {
        return rollback_after(
            RestoreError::StreamChecksumMismatch,
            restore_id,
            resource,
            target,
        );
    }
    if artifact_size_bytes != evidence.artifact_size_bytes() {
        return rollback_after(
            RestoreError::StreamSizeMismatch,
            restore_id,
            resource,
            target,
        );
    }

    if let Err(error) = target.verify(restore_id, resource) {
        return rollback_after(RestoreError::Target(error), restore_id, resource, target);
    }
    if let Err(error) = target.commit(restore_id, resource) {
        return rollback_after(RestoreError::Target(error), restore_id, resource, target);
    }

    Ok(evidence)
}

fn rollback_after(
    primary: RestoreError,
    restore_id: &str,
    resource: &ResourceRecord,
    target: &mut dyn RestoreTarget,
) -> Result<VerifiedBackupEvidence, RestoreError> {
    match target.rollback(restore_id, resource) {
        Ok(()) => Err(primary),
        Err(rollback) => Err(RestoreError::Rollback {
            primary: Box::new(primary),
            rollback,
        }),
    }
}
