use super::LocalCertificateError;
use std::fs::{self, File, OpenOptions};
use std::path::Path;

pub(super) const CERTIFICATE_STORE_LOCK_FILE: &str = ".store.lock";

/// Held exclusive advisory lock for one certificate-generation transaction.
pub(crate) struct CertificateStoreLock {
    _file: File,
}

impl CertificateStoreLock {
    #[cfg(unix)]
    pub(super) fn acquire(root: &Path) -> Result<Self, LocalCertificateError> {
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
        let path = root.join(CERTIFICATE_STORE_LOCK_FILE);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .open(&path)
            .map_err(|error| {
                LocalCertificateError::new(format!(
                    "failed to open certificate store lock '{}': {error}",
                    path.display()
                ))
            })?;
        file.lock().map_err(|error| {
            LocalCertificateError::new(format!(
                "failed to acquire certificate store lock '{}': {error}",
                path.display()
            ))
        })?;

        Ok(Self { _file: file })
    }
}
