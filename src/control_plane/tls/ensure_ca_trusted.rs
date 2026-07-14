use super::{CertificateTrustStore, LocalCaIdentity, TrustChange, TrustStoreError};
use std::path::Path;

/// Installs one exact Stackctl CA only when the OS trust store lacks it.
pub(crate) fn ensure_ca_trusted(
    store: &impl CertificateTrustStore,
    identity: &LocalCaIdentity,
    certificate_path: &Path,
) -> Result<TrustChange, TrustStoreError> {
    if store.contains(identity, certificate_path)? {
        return Ok(TrustChange::Unchanged);
    }

    if let Err(failure) = store.install(identity, certificate_path) {
        if let Err(rollback) = rollback_partial_install(store, identity, certificate_path) {
            return Err(TrustStoreError::new(format!(
                "trust installation failed: {failure}; partial-install rollback failed: {rollback}"
            )));
        }

        return Err(failure);
    }

    Ok(TrustChange::Installed)
}

fn rollback_partial_install(
    store: &impl CertificateTrustStore,
    identity: &LocalCaIdentity,
    certificate_path: &Path,
) -> Result<(), TrustStoreError> {
    match store.contains(identity, certificate_path) {
        Ok(false) => return Ok(()),
        Ok(true) | Err(_) => store.remove(identity, certificate_path)?,
    }
    if store.contains(identity, certificate_path)? {
        return Err(TrustStoreError::new(
            "Stackctl CA remains trusted after partial-install rollback",
        ));
    }

    Ok(())
}
