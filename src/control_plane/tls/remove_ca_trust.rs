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

    store.remove(identity, certificate_path)?;

    Ok(TrustChange::Removed)
}
