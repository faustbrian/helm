use super::{LocalCaIdentity, TrustStoreError};
use std::path::Path;

/// Narrow OS boundary for exact local-CA trust operations.
pub(crate) trait CertificateTrustStore {
    fn contains(
        &self,
        identity: &LocalCaIdentity,
        certificate_path: &Path,
    ) -> Result<bool, TrustStoreError>;

    fn install(
        &self,
        identity: &LocalCaIdentity,
        certificate_path: &Path,
    ) -> Result<(), TrustStoreError>;

    fn remove(
        &self,
        identity: &LocalCaIdentity,
        certificate_path: &Path,
    ) -> Result<(), TrustStoreError>;
}
