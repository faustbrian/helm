use super::{LocalCertificateError, open_certificate_lock_file::open_certificate_lock_file};
use std::fs::File;
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
    open_certificate_lock_file(root, CERTIFICATE_ROTATION_LOCK_FILE)
}
