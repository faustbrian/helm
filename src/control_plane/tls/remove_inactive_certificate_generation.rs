use super::{FilesystemCertificateStore, LocalCertificateError, StoredCertificatePaths};
use std::fs::{self, File};

/// Removes one verified generation only after another generation is active.
#[cfg(unix)]
pub(super) fn remove_inactive_certificate_generation(
    store: &FilesystemCertificateStore,
    inactive_paths: &StoredCertificatePaths,
) -> Result<(), LocalCertificateError> {
    let Some((_active_bundle, active_paths)) = store.load_current()? else {
        return Err(LocalCertificateError::new(
            "cannot remove a certificate generation without an active bundle",
        ));
    };
    if active_paths == *inactive_paths {
        return Err(LocalCertificateError::new(
            "cannot remove the active certificate generation",
        ));
    }
    store.load_directory(inactive_paths.directory())?;
    let root = inactive_paths.directory().parent().ok_or_else(|| {
        LocalCertificateError::new("inactive certificate generation has no root directory")
    })?;
    fs::remove_dir_all(inactive_paths.directory()).map_err(|error| {
        LocalCertificateError::new(format!(
            "failed to remove inactive certificate generation '{}': {error}",
            inactive_paths.directory().display()
        ))
    })?;
    File::open(root)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            LocalCertificateError::new(format!(
                "failed to sync certificate root '{}': {error}",
                root.display()
            ))
        })
}
