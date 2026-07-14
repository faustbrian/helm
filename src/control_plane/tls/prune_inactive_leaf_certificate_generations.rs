use super::{FilesystemCertificateStore, LocalCertificateError};
use std::fs::{self, File};

/// Removes superseded leaf generations after the active one is gateway-ready.
#[cfg(unix)]
pub(crate) fn prune_inactive_leaf_certificate_generations(
    store: &FilesystemCertificateStore,
) -> Result<(), LocalCertificateError> {
    let Some((active_bundle, active_paths)) = store.load_current()? else {
        return Err(LocalCertificateError::new(
            "cannot prune certificate generations without an active bundle",
        ));
    };
    let root = active_paths.directory().parent().ok_or_else(|| {
        LocalCertificateError::new("active certificate generation has no root directory")
    })?;
    let entries = fs::read_dir(root).map_err(|error| {
        LocalCertificateError::new(format!(
            "failed to read certificate root '{}': {error}",
            root.display()
        ))
    })?;
    let mut removed = false;

    for entry in entries {
        let entry = entry.map_err(|error| {
            LocalCertificateError::new(format!(
                "failed to read certificate root entry '{}': {error}",
                root.display()
            ))
        })?;
        let path = entry.path();
        if path == active_paths.directory()
            || !entry
                .file_type()
                .map_err(|error| {
                    LocalCertificateError::new(format!(
                        "failed to inspect certificate root entry '{}': {error}",
                        path.display()
                    ))
                })?
                .is_dir()
        {
            continue;
        }
        let (bundle, _) = store.load_directory(&path)?;
        if bundle.ca_certificate_pem() != active_bundle.ca_certificate_pem() {
            continue;
        }
        fs::remove_dir_all(&path).map_err(|error| {
            LocalCertificateError::new(format!(
                "failed to remove inactive certificate generation '{}': {error}",
                path.display()
            ))
        })?;
        removed = true;
    }

    if removed {
        File::open(root)
            .and_then(|directory| directory.sync_all())
            .map_err(|error| {
                LocalCertificateError::new(format!(
                    "failed to sync certificate root '{}': {error}",
                    root.display()
                ))
            })?;
    }

    Ok(())
}
