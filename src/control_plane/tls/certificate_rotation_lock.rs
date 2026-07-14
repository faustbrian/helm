use super::LocalCertificateError;
use std::fs::{self, File, OpenOptions};
use std::path::Path;

pub(super) const CERTIFICATE_ROTATION_LOCK_FILE: &str = ".rotation.lock";

/// Held advisory lock coordinating trust operations across CA rotation.
pub(crate) struct CertificateRotationLock {
    _file: File,
}

impl CertificateRotationLock {
    #[cfg(unix)]
    pub(super) fn acquire_shared(root: &Path) -> Result<Self, LocalCertificateError> {
        let file = open_lock_file(root)?;
        file.lock_shared().map_err(|error| {
            LocalCertificateError::new(format!(
                "failed to acquire shared certificate rotation lock: {error}"
            ))
        })?;

        Ok(Self { _file: file })
    }

    #[cfg(unix)]
    pub(super) fn acquire_exclusive(root: &Path) -> Result<Self, LocalCertificateError> {
        let file = open_lock_file(root)?;
        file.lock().map_err(|error| {
            LocalCertificateError::new(format!(
                "failed to acquire exclusive certificate rotation lock: {error}"
            ))
        })?;

        Ok(Self { _file: file })
    }
}

#[cfg(unix)]
fn open_lock_file(root: &Path) -> Result<File, LocalCertificateError> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    fs::create_dir_all(root).map_err(|error| {
        LocalCertificateError::new(format!(
            "failed to create certificate root '{}': {error}",
            root.display()
        ))
    })?;
    fs::set_permissions(root, fs::Permissions::from_mode(0o700)).map_err(|error| {
        LocalCertificateError::new(format!(
            "failed to restrict certificate root '{}': {error}",
            root.display()
        ))
    })?;
    let path = root.join(CERTIFICATE_ROTATION_LOCK_FILE);
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .open(&path)
        .map_err(|error| {
            LocalCertificateError::new(format!(
                "failed to open certificate rotation lock '{}': {error}",
                path.display()
            ))
        })
}
