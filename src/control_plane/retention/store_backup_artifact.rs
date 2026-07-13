use super::{BackupArtifactManifest, BackupVerificationError, StoredBackupArtifact};
use crate::control_plane::state::ResourceRecord;
use sha2::{Digest, Sha256};
use std::path::Path;

/// Atomically stores one immutable backup artifact and manifest recovery point.
#[cfg(unix)]
pub(crate) fn store_backup_artifact(
    resource: &ResourceRecord,
    artifact: &[u8],
    created_at_unix_seconds: i64,
    root: &Path,
) -> Result<StoredBackupArtifact, BackupVerificationError> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    let manifest =
        BackupArtifactManifest::from_artifact(resource, artifact, created_at_unix_seconds)?;
    let mut manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|error| {
        BackupVerificationError::InvalidManifest {
            detail: format!("failed to encode backup manifest: {error}"),
        }
    })?;
    manifest_bytes.push(b'\n');

    prepare_private_directory(root)?;
    let identity_hash = backup_identity_hash(resource);
    let resource_directory = root.join(identity_hash);
    prepare_private_directory(&resource_directory)?;
    let destination = resource_directory.join(format!(
        "{}-{}",
        created_at_unix_seconds,
        manifest.artifact_sha256()
    ));
    let stored = StoredBackupArtifact::new(&destination);
    if destination.exists() {
        let existing_manifest = fs::read(stored.manifest_file()).map_err(|error| {
            storage_error(
                "read existing backup manifest",
                stored.manifest_file(),
                error,
            )
        })?;
        let existing_artifact = fs::read(stored.artifact_file()).map_err(|error| {
            storage_error(
                "read existing backup artifact",
                stored.artifact_file(),
                error,
            )
        })?;
        if existing_manifest == manifest_bytes && existing_artifact == artifact {
            return Ok(stored);
        }

        return Err(BackupVerificationError::Storage {
            detail: format!(
                "existing backup recovery point '{}' does not match the requested artifact",
                destination.display()
            ),
        });
    }

    let pending = resource_directory.join(format!(
        ".pending-{}-{}",
        created_at_unix_seconds,
        manifest.artifact_sha256()
    ));
    remove_incomplete_pending_directory(&pending)?;
    fs::create_dir(&pending)
        .map_err(|error| storage_error("create pending backup directory", &pending, error))?;
    fs::set_permissions(&pending, fs::Permissions::from_mode(0o700))
        .map_err(|error| storage_error("protect pending backup directory", &pending, error))?;

    let pending_stored = StoredBackupArtifact::new(&pending);
    write_private_file(pending_stored.artifact_file(), artifact)?;
    write_private_file(pending_stored.manifest_file(), &manifest_bytes)?;
    fs::File::open(&pending)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| storage_error("sync pending backup directory", &pending, error))?;
    fs::rename(&pending, &destination)
        .map_err(|error| storage_error("publish backup recovery point", &destination, error))?;
    fs::File::open(&resource_directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            storage_error("sync backup resource directory", &resource_directory, error)
        })?;

    Ok(stored)
}

#[cfg(not(unix))]
pub(crate) fn store_backup_artifact(
    _resource: &ResourceRecord,
    _artifact: &[u8],
    _created_at_unix_seconds: i64,
    root: &Path,
) -> Result<StoredBackupArtifact, BackupVerificationError> {
    Err(BackupVerificationError::Storage {
        detail: format!(
            "secure backup persistence is not implemented for '{}' on this platform",
            root.display()
        ),
    })
}

fn backup_identity_hash(resource: &ResourceRecord) -> String {
    let mut digest = Sha256::new();
    digest.update(b"stackctl-backup-resource-v1\0");
    for value in [
        resource.installation_id(),
        resource.resource_id(),
        resource.compatibility_fingerprint(),
    ] {
        digest.update(value.as_bytes());
        digest.update(b"\0");
    }

    hex::encode(digest.finalize())
}

#[cfg(unix)]
fn write_private_file(path: &Path, contents: &[u8]) -> Result<(), BackupVerificationError> {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| storage_error("create private backup file", path, error))?;
    file.write_all(contents)
        .map_err(|error| storage_error("write private backup file", path, error))?;
    file.sync_all()
        .map_err(|error| storage_error("sync private backup file", path, error))
}

#[cfg(unix)]
fn prepare_private_directory(path: &Path) -> Result<(), BackupVerificationError> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fs::create_dir_all(path)
        .map_err(|error| storage_error("create backup directory", path, error))?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| storage_error("inspect backup directory", path, error))?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(BackupVerificationError::Storage {
            detail: format!("backup path '{}' must be a real directory", path.display()),
        });
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| storage_error("protect backup directory", path, error))
}

#[cfg(unix)]
fn remove_incomplete_pending_directory(path: &Path) -> Result<(), BackupVerificationError> {
    use std::fs;

    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(storage_error(
                "inspect pending backup directory",
                path,
                error,
            ));
        }
    };
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(BackupVerificationError::Storage {
            detail: format!(
                "pending backup path '{}' must be a real directory",
                path.display()
            ),
        });
    }

    fs::remove_dir_all(path)
        .map_err(|error| storage_error("remove incomplete pending backup", path, error))
}

#[cfg(unix)]
fn storage_error(action: &str, path: &Path, error: std::io::Error) -> BackupVerificationError {
    BackupVerificationError::Storage {
        detail: format!("failed to {action} '{}': {error}", path.display()),
    }
}
