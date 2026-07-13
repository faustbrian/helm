use super::{
    LocalCertificateBundle, LocalCertificateError, generate_leaf_certificate, generation_error,
};
use rcgen::{Issuer, KeyPair};
use time::OffsetDateTime;

/// Renews gateway leaf material while preserving the trusted Stackctl CA.
pub(crate) fn renew_local_leaf_certificate(
    current: &LocalCertificateBundle,
    now: OffsetDateTime,
) -> Result<LocalCertificateBundle, LocalCertificateError> {
    let ca_key = KeyPair::from_pem(current.ca_private_key_pem())
        .map_err(|error| generation_error("parse persisted CA key", error))?;
    let issuer = Issuer::from_ca_cert_pem(current.ca_certificate_pem(), ca_key)
        .map_err(|error| generation_error("parse persisted CA certificate", error))?;
    let leaf = generate_leaf_certificate(&issuer, now)?;

    Ok(LocalCertificateBundle::new(
        current.ca_certificate_pem().to_owned(),
        current.ca_private_key_pem().to_owned(),
        leaf.certificate_pem,
        leaf.private_key_pem,
        leaf.renew_after,
    ))
}
