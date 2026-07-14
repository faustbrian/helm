use super::{
    CertificateTrustStore, CurrentCaTrustResult, FilesystemCertificateStore, LocalCaIdentity,
    LocalCaTrustError, remove_ca_trust,
};

/// Removes trust only when exact persisted Stackctl CA material is available.
pub(crate) fn remove_current_ca_trust(
    certificates: &FilesystemCertificateStore,
    trust_store: &impl CertificateTrustStore,
) -> Result<Option<CurrentCaTrustResult>, LocalCaTrustError> {
    let _lock = certificates.lock()?;
    let Some((bundle, paths)) = certificates.load_current()? else {
        return Ok(None);
    };
    let identity = LocalCaIdentity::from_pem(bundle.ca_certificate_pem())?;
    let change = remove_ca_trust(trust_store, &identity, &paths.ca_certificate())?;

    Ok(Some(CurrentCaTrustResult::new(identity, change)))
}
