use super::{LocalCertificateError, open_certificate_lock_file::open_certificate_lock_file};
use std::fs::File;
use std::path::Path;

pub(super) const CERTIFICATE_STORE_LOCK_FILE: &str = ".store.lock";

/// Held exclusive advisory lock for one certificate-generation transaction.
pub(crate) struct CertificateStoreLock {
    _file: File,
}

impl CertificateStoreLock {
    #[cfg(unix)]
    pub(super) fn acquire(root: &Path) -> Result<Self, LocalCertificateError> {
        let file = open_certificate_lock_file(root, CERTIFICATE_STORE_LOCK_FILE)?;
        file.lock().map_err(|error| {
            LocalCertificateError::new(format!(
                "failed to acquire certificate store lock under '{}': {error}",
                root.display()
            ))
        })?;

        Ok(Self { _file: file })
    }
}
