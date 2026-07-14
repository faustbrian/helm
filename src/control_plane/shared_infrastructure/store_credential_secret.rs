use super::{CredentialSecret, ManagedSecretStoreError};
use std::path::{Path, PathBuf};

/// Atomically stores one immutable daemon credential in a user-private file.
#[cfg(unix)]
pub(crate) fn store_credential_secret(
    secret: &CredentialSecret,
    path: &Path,
) -> Result<PathBuf, ManagedSecretStoreError> {
    use std::fs::{self, File, OpenOptions};
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    if secret.expose().is_empty() || secret.expose().contains('\0') {
        return Err(ManagedSecretStoreError::new(
            "managed secrets must be non-empty and contain no NUL bytes",
        ));
    }
    let directory = path.parent().ok_or_else(|| {
        ManagedSecretStoreError::new(format!(
            "managed secret path '{}' must have a parent directory",
            path.display()
        ))
    })?;
    fs::create_dir_all(directory)
        .map_err(|error| io_error("create secret directory", directory, error))?;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
        .map_err(|error| io_error("restrict secret directory", directory, error))?;

    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            ManagedSecretStoreError::new(format!(
                "managed secret path '{}' must end in a UTF-8 file name",
                path.display()
            ))
        })?;
    let temporary = directory.join(format!(".{name}.tmp"));
    match fs::remove_file(&temporary) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(io_error(
                "remove interrupted managed secret",
                &temporary,
                error,
            ));
        }
    }

    if path.exists() {
        let found = fs::read(path).map_err(|error| io_error("read managed secret", path, error))?;
        if found != secret.expose().as_bytes() {
            return Err(ManagedSecretStoreError::new(format!(
                "refusing to replace existing managed secret '{}'",
                path.display()
            )));
        }
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| io_error("restrict managed secret", path, error))?;
        File::open(directory)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| io_error("sync secret directory", directory, error))?;

        return Ok(path.to_path_buf());
    }

    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(|error| io_error("create temporary managed secret", &temporary, error))?;
    file.write_all(secret.expose().as_bytes())
        .map_err(|error| io_error("write temporary managed secret", &temporary, error))?;
    file.sync_all()
        .map_err(|error| io_error("sync temporary managed secret", &temporary, error))?;
    fs::rename(&temporary, path)
        .map_err(|error| io_error("publish managed secret", path, error))?;
    File::open(directory)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| io_error("sync secret directory", directory, error))?;

    Ok(path.to_path_buf())
}

#[cfg(unix)]
fn io_error(action: &str, path: &Path, error: std::io::Error) -> ManagedSecretStoreError {
    ManagedSecretStoreError::new(format!("failed to {action} '{}': {error}", path.display()))
}
