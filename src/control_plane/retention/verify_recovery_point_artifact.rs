use super::{
    BackupResourceIdentity, BackupVerificationError, open_stored_backup_artifact,
    verify_stored_backup_artifact,
};
use crate::control_plane::state::{LogicalResourceRecord, RecoveryPointRecord};

/// Rereads and binds one cataloged recovery point immediately before deletion.
pub(crate) fn verify_recovery_point_artifact(
    recovery: &RecoveryPointRecord,
    logical: &LogicalResourceRecord,
    installation_id: &str,
    verified_at_unix_seconds: i64,
) -> Result<(), BackupVerificationError> {
    let exact_catalog_identity = recovery.project_id() == logical.project_id()
        && recovery.service_id() == logical.service_id()
        && recovery.logical_resource_id() == logical.logical_resource_id()
        && recovery.resource_kind() == logical.kind()
        && recovery.compatibility_fingerprint() == logical.compatibility_fingerprint();
    if !exact_catalog_identity || installation_id.is_empty() {
        return Err(BackupVerificationError::InvalidManifest {
            detail: "recovery point does not match the exact logical resource".to_owned(),
        });
    }
    let stored = open_stored_backup_artifact(recovery.reference())?;
    let evidence = verify_stored_backup_artifact(&stored, verified_at_unix_seconds)?;
    let identity = BackupResourceIdentity::from_logical(logical, installation_id);
    if !evidence.matches_identity(&identity) {
        return Err(BackupVerificationError::InvalidManifest {
            detail: "backup manifest does not match the exact logical resource".to_owned(),
        });
    }
    if evidence.artifact_sha256() != recovery.artifact_sha256() {
        return Err(BackupVerificationError::ChecksumMismatch);
    }
    if evidence.artifact_size_bytes() != recovery.artifact_size_bytes() {
        return Err(BackupVerificationError::SizeMismatch);
    }

    Ok(())
}
