use super::{CertificateTrustStore, LocalCaIdentity, TrustChange, TrustStoreError};
use std::path::Path;

/// Removes only an exact Stackctl CA identity that is currently installed.
pub(crate) fn remove_ca_trust(
    store: &impl CertificateTrustStore,
    identity: &LocalCaIdentity,
    certificate_path: &Path,
) -> Result<TrustChange, TrustStoreError> {
    if !store.contains(identity, certificate_path)? {
        return Ok(TrustChange::Unchanged);
    }

    if let Err(failure) = store.remove(identity, certificate_path) {
        if let Err(rollback) = rollback_partial_removal(store, identity, certificate_path) {
            return Err(TrustStoreError::new(format!(
                "trust removal failed: {failure}; partial-removal rollback failed: {rollback}"
            )));
        }

        return Err(failure);
    }

    Ok(TrustChange::Removed)
}

fn rollback_partial_removal(
    store: &impl CertificateTrustStore,
    identity: &LocalCaIdentity,
    certificate_path: &Path,
) -> Result<(), TrustStoreError> {
    match store.contains(identity, certificate_path) {
        Ok(true) => return Ok(()),
        Ok(false) | Err(_) => {
            if let Err(install_failure) = store.install(identity, certificate_path)
                && !store.contains(identity, certificate_path)?
            {
                return Err(install_failure);
            }
        }
    }
    if !store.contains(identity, certificate_path)? {
        return Err(TrustStoreError::new(
            "Stackctl CA remains untrusted after partial-removal rollback",
        ));
    }

    Ok(())
}
