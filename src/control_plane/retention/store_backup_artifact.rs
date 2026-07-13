use super::{
    BackupArtifactManifest, BackupResourceIdentity, BackupVerificationError, StoredBackupArtifact,
    verify_stored_backup_artifact,
};
use crate::control_plane::state::ResourceRecord;
use sha2::{Digest, Sha256};
use std::io::{Cursor, Read};
use std::path::Path;
#[cfg(unix)]
use std::path::PathBuf;

/// Atomically stores one in-memory artifact through the streaming store.
pub(crate) fn store_backup_artifact(
    resource: &ResourceRecord,
    artifact: &[u8],
    created_at_unix_seconds: i64,
    root: &Path,
) -> Result<StoredBackupArtifact, BackupVerificationError> {
    store_backup_artifact_for_identity(
        &BackupResourceIdentity::from_resource(resource),
        artifact,
        created_at_unix_seconds,
        root,
    )
}

/// Atomically stores bytes for an Engine or logical resource identity.
pub(crate) fn store_backup_artifact_for_identity(
    resource: &BackupResourceIdentity,
    artifact: &[u8],
    created_at_unix_seconds: i64,
    root: &Path,
) -> Result<StoredBackupArtifact, BackupVerificationError> {
    store_backup_artifact_from_reader_for_identity(
        resource,
        Cursor::new(artifact),
        created_at_unix_seconds,
        root,
    )
}

/// Streams one immutable backup and atomically publishes its manifest pair.
#[cfg(unix)]
pub(crate) fn store_backup_artifact_from_reader(
    resource: &ResourceRecord,
    artifact: impl Read,
    created_at_unix_seconds: i64,
    root: &Path,
) -> Result<StoredBackupArtifact, BackupVerificationError> {
    store_backup_artifact_from_reader_for_identity(
        &BackupResourceIdentity::from_resource(resource),
        artifact,
        created_at_unix_seconds,
        root,
    )
}

#[cfg(unix)]
fn store_backup_artifact_from_reader_for_identity(
    resource: &BackupResourceIdentity,
    mut artifact: impl Read,
    created_at_unix_seconds: i64,
    root: &Path,
) -> Result<StoredBackupArtifact, BackupVerificationError> {
    use std::fs;

    if created_at_unix_seconds < 0 {
        return Err(BackupVerificationError::InvalidCreationTime);
    }
    let (resource_directory, pending, pending_stored) =
        prepare_pending_backup(resource, created_at_unix_seconds, root)?;
    let (artifact_sha256, artifact_size_bytes) =
        write_private_artifact(pending_stored.artifact_file(), &mut artifact)?;
    let manifest = BackupArtifactManifest::from_checksum(
        resource,
        artifact_sha256,
        artifact_size_bytes,
        created_at_unix_seconds,
    )?;
    let manifest_bytes = encode_manifest(&manifest)?;
    write_private_file(pending_stored.manifest_file(), &manifest_bytes)?;
    fs::File::open(&pending)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| storage_error("sync pending backup directory", &pending, error))?;

    publish_pending_backup(
        resource,
        created_at_unix_seconds,
        &resource_directory,
        &pending,
        &manifest,
        &manifest_bytes,
    )
}

#[cfg(not(unix))]
pub(crate) fn store_backup_artifact_from_reader(
    _resource: &ResourceRecord,
    _artifact: impl Read,
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

#[cfg(not(unix))]
fn store_backup_artifact_from_reader_for_identity(
    _resource: &BackupResourceIdentity,
    _artifact: impl Read,
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

fn backup_identity_hash(resource: &BackupResourceIdentity) -> String {
    let mut digest = Sha256::new();
    digest.update(b"stackctl-backup-resource-v1\0");
    for value in [
        resource.installation_id(),
        resource.resource_id(),
        resource.resource_kind(),
        resource.compatibility_fingerprint(),
    ] {
        digest.update(value.as_bytes());
        digest.update(b"\0");
    }

    hex::encode(digest.finalize())
}

#[cfg(unix)]
fn write_private_artifact(
    path: &Path,
    artifact: &mut impl Read,
) -> Result<(String, u64), BackupVerificationError> {
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|error| storage_error("create private backup artifact", path, error))?;
    let mut digest = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = artifact
            .read(&mut buffer)
            .map_err(|error| storage_error("read backup input", path, error))?;
        if count == 0 {
            break;
        }
        file.write_all(&buffer[..count])
            .map_err(|error| storage_error("write private backup artifact", path, error))?;
        digest.update(&buffer[..count]);
        size = size.saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
    }
    if size == 0 {
        return Err(BackupVerificationError::EmptyArtifact);
    }
    file.sync_all()
        .map_err(|error| storage_error("sync private backup artifact", path, error))?;

    Ok((hex::encode(digest.finalize()), size))
}

#[cfg(unix)]
pub(super) fn write_private_file(
    path: &Path,
    contents: &[u8],
) -> Result<(), BackupVerificationError> {
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
pub(super) fn storage_error(
    action: &str,
    path: &Path,
    error: std::io::Error,
) -> BackupVerificationError {
    BackupVerificationError::Storage {
        detail: format!("failed to {action} '{}': {error}", path.display()),
    }
}

#[cfg(unix)]
pub(super) fn prepare_pending_backup(
    resource: &BackupResourceIdentity,
    created_at_unix_seconds: i64,
    root: &Path,
) -> Result<(PathBuf, PathBuf, StoredBackupArtifact), BackupVerificationError> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    if created_at_unix_seconds < 0 {
        return Err(BackupVerificationError::InvalidCreationTime);
    }
    prepare_private_directory(root)?;
    let resource_directory = root.join(backup_identity_hash(resource));
    prepare_private_directory(&resource_directory)?;
    let pending = resource_directory.join(format!(".pending-{created_at_unix_seconds}"));
    remove_incomplete_pending_directory(&pending)?;
    fs::create_dir(&pending)
        .map_err(|error| storage_error("create pending backup directory", &pending, error))?;
    fs::set_permissions(&pending, fs::Permissions::from_mode(0o700))
        .map_err(|error| storage_error("protect pending backup directory", &pending, error))?;
    let pending_stored = StoredBackupArtifact::new(&pending);

    Ok((resource_directory, pending, pending_stored))
}

pub(super) fn encode_manifest(
    manifest: &BackupArtifactManifest,
) -> Result<Vec<u8>, BackupVerificationError> {
    let mut bytes = serde_json::to_vec_pretty(manifest).map_err(|error| {
        BackupVerificationError::InvalidManifest {
            detail: format!("failed to encode backup manifest: {error}"),
        }
    })?;
    bytes.push(b'\n');

    Ok(bytes)
}

#[cfg(unix)]
pub(super) fn publish_pending_backup(
    resource: &BackupResourceIdentity,
    created_at_unix_seconds: i64,
    resource_directory: &Path,
    pending: &Path,
    manifest: &BackupArtifactManifest,
    manifest_bytes: &[u8],
) -> Result<StoredBackupArtifact, BackupVerificationError> {
    use std::fs;

    let destination = resource_directory.join(format!(
        "{}-{}",
        created_at_unix_seconds,
        manifest.artifact_sha256()
    ));
    let stored = StoredBackupArtifact::new(&destination);
    if destination.exists() {
        let evidence = verify_stored_backup_artifact(&stored, created_at_unix_seconds)?;
        if !evidence.matches_identity(resource) {
            return Err(BackupVerificationError::Storage {
                detail: format!(
                    "existing backup recovery point '{}' belongs to another resource",
                    destination.display()
                ),
            });
        }
        let existing_manifest = fs::read(stored.manifest_file()).map_err(|error| {
            storage_error(
                "read existing backup manifest",
                stored.manifest_file(),
                error,
            )
        })?;
        if existing_manifest != manifest_bytes {
            return Err(BackupVerificationError::Storage {
                detail: format!(
                    "existing backup recovery point '{}' does not match the requested artifact",
                    destination.display()
                ),
            });
        }
        fs::remove_dir_all(pending)
            .map_err(|error| storage_error("discard duplicate pending backup", pending, error))?;

        return Ok(stored);
    }

    fs::rename(pending, &destination)
        .map_err(|error| storage_error("publish backup recovery point", &destination, error))?;
    fs::File::open(resource_directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            storage_error("sync backup resource directory", resource_directory, error)
        })?;

    Ok(stored)
}
