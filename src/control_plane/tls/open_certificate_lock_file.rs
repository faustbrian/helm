use super::LocalCertificateError;
use std::fs::{self, File, OpenOptions};
use std::path::Path;

/// Opens one private real lock file under a private real certificate root.
#[cfg(unix)]
pub(super) fn open_certificate_lock_file(
    root: &Path,
    name: &str,
) -> Result<File, LocalCertificateError> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    fs::create_dir_all(root).map_err(|error| failure("create", root, error))?;
    let metadata = fs::symlink_metadata(root).map_err(|error| failure("inspect", root, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(LocalCertificateError::new(format!(
            "certificate root '{}' must be a real directory",
            root.display()
        )));
    }
    fs::set_permissions(root, fs::Permissions::from_mode(0o700))
        .map_err(|error| failure("restrict", root, error))?;

    let path = root.join(name);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
                .map_err(|error| failure("restrict", &path, error))?;
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .map_err(|error| failure("open", &path, error))
        }
        Ok(_) => Err(LocalCertificateError::new(format!(
            "certificate lock '{}' must be a real file",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(|error| failure("create", &path, error)),
        Err(error) => Err(failure("inspect", &path, error)),
    }
}

fn failure(action: &str, path: &Path, error: std::io::Error) -> LocalCertificateError {
    LocalCertificateError::new(format!(
        "failed to {action} certificate lock path '{}': {error}",
        path.display()
    ))
}
