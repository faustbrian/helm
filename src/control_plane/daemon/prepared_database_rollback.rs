use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// Private operation-scoped storage removed unless manual recovery is required.
pub(crate) struct PreparedDatabaseRollback {
    root: PathBuf,
    cleanup: bool,
}

impl PreparedDatabaseRollback {
    pub(crate) fn prepare(backup_root: &Path, operation_id: &str) -> Result<Self, String> {
        if !backup_root.is_absolute() || operation_id.is_empty() {
            return Err(
                "database rollback storage requires an absolute root and operation identity"
                    .to_owned(),
            );
        }
        let parent = backup_root.join("database-dump-rollbacks");
        prepare_private_directory(&parent)?;
        let root = parent.join(hex::encode(Sha256::digest(operation_id)));
        remove_stale_directory(&root)?;
        fs::create_dir(&root).map_err(|error| {
            format!("database rollback directory could not be created: {error}")
        })?;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(|error| {
            format!("database rollback directory permissions could not be set: {error}")
        })?;

        Ok(Self {
            root,
            cleanup: true,
        })
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn preserve(&mut self) {
        self.cleanup = false;
    }
}

impl Drop for PreparedDatabaseRollback {
    fn drop(&mut self) {
        if self.cleanup {
            drop(fs::remove_dir_all(&self.root));
        }
    }
}

fn prepare_private_directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path)
        .map_err(|error| format!("database rollback storage could not be created: {error}"))?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("database rollback storage could not be inspected: {error}"))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err("database rollback storage must be a real directory".to_owned());
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("database rollback storage permissions failed: {error}"))
}

fn remove_stale_directory(path: &Path) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "stale database rollback directory could not be inspected: {error}"
            ));
        }
    };
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err("stale database rollback path must be a real directory".to_owned());
    }
    fs::remove_dir_all(path)
        .map_err(|error| format!("stale database rollback directory could not be removed: {error}"))
}
