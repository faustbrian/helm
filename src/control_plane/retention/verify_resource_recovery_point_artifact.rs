use super::{
    BackupResourceIdentity, BackupVerificationError, StoredBackupArtifact,
    open_stored_backup_artifact, verify_stored_backup_artifact,
};
use crate::control_plane::state::{RecoveryPointRecord, ResourceRecord};

/// Rereads and binds one cataloged recovery point to an exact physical resource.
pub(crate) fn verify_resource_recovery_point_artifact(
    recovery: &RecoveryPointRecord,
    resource: &ResourceRecord,
    verified_at_unix_seconds: i64,
) -> Result<StoredBackupArtifact, BackupVerificationError> {
    let exact_catalog_identity = resource.project_id().is_some()
        && resource.scope_id().is_some()
        && recovery.project_id() == resource.project_id().unwrap_or_default()
        && recovery.service_id() == resource.scope_id().unwrap_or_default()
        && recovery.logical_resource_id() == resource.resource_id()
        && recovery.resource_kind() == resource.kind()
        && recovery.compatibility_fingerprint() == resource.compatibility_fingerprint();
    if !exact_catalog_identity {
        return Err(BackupVerificationError::InvalidManifest {
            detail: "recovery point does not match the exact physical resource".to_owned(),
        });
    }
    let stored = open_stored_backup_artifact(recovery.reference())?;
    let evidence = verify_stored_backup_artifact(&stored, verified_at_unix_seconds)?;
    let identity = BackupResourceIdentity::from_resource(resource);
    if !evidence.matches_identity(&identity) {
        return Err(BackupVerificationError::InvalidManifest {
            detail: "backup manifest does not match the exact physical resource".to_owned(),
        });
    }
    if evidence.artifact_sha256() != recovery.artifact_sha256() {
        return Err(BackupVerificationError::ChecksumMismatch);
    }
    if evidence.artifact_size_bytes() != recovery.artifact_size_bytes() {
        return Err(BackupVerificationError::SizeMismatch);
    }

    Ok(stored)
}
