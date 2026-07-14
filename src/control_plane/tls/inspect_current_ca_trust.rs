use super::{
    CertificateTrustStore, CurrentCaTrustStatus, FilesystemCertificateStore, LocalCaIdentity,
    LocalCaTrustError,
};

/// Inspects trust without creating or changing certificate material.
pub(crate) fn inspect_current_ca_trust(
    certificates: &FilesystemCertificateStore,
    trust_store: &impl CertificateTrustStore,
) -> Result<CurrentCaTrustStatus, LocalCaTrustError> {
    let _lock = certificates.lock()?;
    let Some((bundle, paths)) = certificates.load_current()? else {
        return Ok(CurrentCaTrustStatus::Absent);
    };
    let identity = LocalCaIdentity::from_pem(bundle.ca_certificate_pem())?;

    if trust_store.contains(&identity, &paths.ca_certificate())? {
        return Ok(CurrentCaTrustStatus::Trusted(identity));
    }

    Ok(CurrentCaTrustStatus::Untrusted(identity))
}
