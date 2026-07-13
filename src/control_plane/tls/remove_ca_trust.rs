use super::{CertificateTrustStore, LocalCaIdentity, TrustChange, TrustStoreError};

/// Removes only an exact Stackctl CA identity that is currently installed.
pub(crate) fn remove_ca_trust(
    store: &impl CertificateTrustStore,
    identity: &LocalCaIdentity,
) -> Result<TrustChange, TrustStoreError> {
    if !store.contains(identity)? {
        return Ok(TrustChange::Unchanged);
    }

    store.remove(identity)?;

    Ok(TrustChange::Removed)
}
