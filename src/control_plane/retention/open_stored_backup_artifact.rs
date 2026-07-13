use super::{BackupVerificationError, StoredBackupArtifact};
use std::path::Path;

/// Opens only an absolute recovery point containing real regular files.
pub(crate) fn open_stored_backup_artifact(
    reference: &str,
) -> Result<StoredBackupArtifact, BackupVerificationError> {
    let recovery_point = Path::new(reference);
    if !recovery_point.is_absolute() {
        return Err(BackupVerificationError::Storage {
            detail: "backup recovery point reference must be absolute".to_owned(),
        });
    }
    require_real_directory(recovery_point)?;
    let stored = StoredBackupArtifact::new(recovery_point);
    require_real_file(stored.artifact_file())?;
    require_real_file(stored.manifest_file())?;

    Ok(stored)
}

fn require_real_directory(path: &Path) -> Result<(), BackupVerificationError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| storage_error(path, error))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(BackupVerificationError::Storage {
            detail: format!(
                "backup recovery point '{}' must be a real directory",
                path.display()
            ),
        });
    }

    Ok(())
}

fn require_real_file(path: &Path) -> Result<(), BackupVerificationError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|error| storage_error(path, error))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(BackupVerificationError::Storage {
            detail: format!("backup path '{}' must be a real file", path.display()),
        });
    }

    Ok(())
}

fn storage_error(path: &Path, error: std::io::Error) -> BackupVerificationError {
    BackupVerificationError::Storage {
        detail: format!(
            "failed to inspect backup path '{}': {error}",
            path.display()
        ),
    }
}
