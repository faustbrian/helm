use super::{CertificateTrustStore, LocalCaIdentity, TrustChange, TrustStoreError};
use std::path::Path;

/// Installs one exact Stackctl CA only when the OS trust store lacks it.
pub(crate) fn ensure_ca_trusted(
    store: &(impl CertificateTrustStore + ?Sized),
    identity: &LocalCaIdentity,
    certificate_path: &Path,
) -> Result<TrustChange, TrustStoreError> {
    if store.contains(identity, certificate_path)? {
        return Ok(TrustChange::Unchanged);
    }

    store.install(identity, certificate_path)?;

    Ok(TrustChange::Installed)
}
