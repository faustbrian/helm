use super::{
    CertificateTrustStore, CurrentCaTrustResult, FilesystemCertificateStore, LocalCaIdentity,
    LocalCaTrustError, ensure_ca_trusted, reconcile_local_certificates,
};
use time::OffsetDateTime;

/// Recovers or creates the singleton CA and installs its exact public identity.
pub(crate) fn install_current_ca_trust(
    certificates: &FilesystemCertificateStore,
    trust_store: &impl CertificateTrustStore,
    now: OffsetDateTime,
) -> Result<CurrentCaTrustResult, LocalCaTrustError> {
    let current = certificates.load_current()?;
    let reconciliation =
        reconcile_local_certificates(current.as_ref().map(|(bundle, _)| bundle), now)?;
    let paths = certificates.persist_inactive(reconciliation.bundle())?;
    let identity = LocalCaIdentity::from_pem(reconciliation.bundle().ca_certificate_pem())?;
    let change = ensure_ca_trusted(trust_store, &identity, &paths.ca_certificate())?;
    if !trust_store.contains(&identity, &paths.ca_certificate())? {
        return Err(
            super::TrustStoreError::new("Stackctl CA is not trusted after installation").into(),
        );
    }
    certificates.activate(&paths)?;

    Ok(CurrentCaTrustResult::new(identity, change))
}
