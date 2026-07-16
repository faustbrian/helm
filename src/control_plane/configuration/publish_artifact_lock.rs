use super::{
    ArtifactLock, ArtifactLockPublicationError, MAX_PROJECT_CONFIG_BYTES, parse_artifact_lock,
};
use crate::control_plane::lock_directory;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Atomically replaces an artifact lock after serializing and validating it.
pub(crate) fn replace_artifact_lock(
    path: &Path,
    lock: &ArtifactLock,
) -> Result<(), ArtifactLockPublicationError> {
    publish(path, lock, true, None).map(|_| ())
}

/// Replaces a generated lock only when its contents still match discovery.
pub(crate) fn replace_artifact_lock_if_unchanged(
    path: &Path,
    expected: &str,
    lock: &ArtifactLock,
) -> Result<bool, ArtifactLockPublicationError> {
    publish(path, lock, true, Some(expected))
}

/// Atomically creates a missing artifact lock without replacing any path.
pub(crate) fn publish_missing_artifact_lock(
    path: &Path,
    lock: &ArtifactLock,
) -> Result<bool, ArtifactLockPublicationError> {
    publish(path, lock, false, None)
}

fn publish(
    path: &Path,
    lock: &ArtifactLock,
    replace_existing: bool,
    expected_existing: Option<&str>,
) -> Result<bool, ArtifactLockPublicationError> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid("artifact lock path has no parent directory"))?;
    let directory_lock = lock_directory(parent).map_err(|error| {
        invalid(format!(
            "failed to lock real artifact lock directory {}: {error}",
            parent.display()
        ))
    })?;
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(invalid(format!(
            "refusing to replace symbolic-link artifact lock {}",
            path.display()
        )));
    }
    if !replace_existing && path.exists() {
        return Ok(false);
    }
    if let Some(expected) = expected_existing {
        let current = fs::read_to_string(path).map_err(|error| {
            invalid(format!(
                "failed to verify existing artifact lock {}: {error}",
                path.display()
            ))
        })?;
        if current != expected {
            return Ok(false);
        }
    }

    let yaml = serde_yaml_ng::to_string(lock)
        .map_err(|error| invalid(format!("failed to serialize v8 artifact lock: {error}")))?;
    if yaml.len() > MAX_PROJECT_CONFIG_BYTES {
        return Err(invalid("generated artifact lock exceeds the size limit"));
    }
    parse_artifact_lock(&yaml, path).map_err(|error| {
        invalid(format!(
            "generated artifact lock failed validation: {error}"
        ))
    })?;
    let temporary = temporary_path(path);
    remove_stale_temporary(&temporary, parent, &directory_lock)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| {
            invalid(format!(
                "failed to create temporary lock {}: {error}",
                temporary.display()
            ))
        })?;
    let result = (|| {
        file.write_all(yaml.as_bytes()).map_err(|error| {
            invalid(format!(
                "failed to write temporary lock {}: {error}",
                temporary.display()
            ))
        })?;
        file.sync_all().map_err(|error| {
            invalid(format!(
                "failed to sync temporary lock {}: {error}",
                temporary.display()
            ))
        })?;
        if replace_existing {
            fs::rename(&temporary, path).map_err(|error| {
                invalid(format!(
                    "failed to publish artifact lock {}: {error}",
                    path.display()
                ))
            })?;
        } else {
            match fs::hard_link(&temporary, path) {
                Ok(()) => fs::remove_file(&temporary).map_err(|error| {
                    invalid(format!(
                        "failed to remove published staging lock {}: {error}",
                        temporary.display()
                    ))
                })?,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    fs::remove_file(&temporary).map_err(|remove_error| {
                        invalid(format!(
                            "failed to remove unused staging lock {}: {remove_error}",
                            temporary.display()
                        ))
                    })?;
                    return Ok(false);
                }
                Err(error) => {
                    return Err(invalid(format!(
                        "failed to publish missing artifact lock {}: {error}",
                        path.display()
                    )));
                }
            }
        }
        directory_lock.sync_all().map_err(|error| {
            invalid(format!(
                "failed to sync artifact lock directory {}: {error}",
                parent.display()
            ))
        })?;
        Ok(true)
    })();
    if result.is_err() {
        drop(fs::remove_file(&temporary));
    }

    result
}

fn remove_stale_temporary(
    temporary: &Path,
    parent: &Path,
    directory: &fs::File,
) -> Result<(), ArtifactLockPublicationError> {
    match fs::remove_file(temporary) {
        Ok(()) => directory.sync_all().map_err(|error| {
            invalid(format!(
                "failed to sync artifact lock directory {}: {error}",
                parent.display()
            ))
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(invalid(format!(
            "failed to remove stale temporary lock {}: {error}",
            temporary.display()
        ))),
    }
}

fn temporary_path(path: &Path) -> PathBuf {
    path.with_extension("yaml.tmp")
}

fn invalid(detail: impl Into<String>) -> ArtifactLockPublicationError {
    ArtifactLockPublicationError::new(detail)
}
