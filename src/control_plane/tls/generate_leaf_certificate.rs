use super::{LocalCertificateError, checked_time, generation_error};
use rcgen::{
    CertificateParams, DnType, ExtendedKeyUsagePurpose, Issuer, KeyPair, KeyUsagePurpose,
    SigningKey,
};
use time::OffsetDateTime;

const LEAF_VALIDITY_DAYS: i64 = 90;
const LEAF_RENEWAL_DAYS: i64 = 75;

pub(super) struct LeafCertificateMaterial {
    pub(super) certificate_pem: String,
    pub(super) private_key_pem: String,
    pub(super) renew_after: OffsetDateTime,
}

/// Issues a fresh wildcard gateway leaf from one Stackctl CA issuer.
pub(super) fn generate_leaf_certificate(
    issuer: &Issuer<'_, impl SigningKey>,
    now: OffsetDateTime,
) -> Result<LeafCertificateMaterial, LocalCertificateError> {
    let not_before = checked_time(now, -1, "certificate not-before")?;
    let not_after = checked_time(now, LEAF_VALIDITY_DAYS, "leaf expiry")?;
    let renew_after = checked_time(now, LEAF_RENEWAL_DAYS, "leaf renewal")?;
    let mut parameters = CertificateParams::new(vec!["*.stackctl.localhost".to_owned()])
        .map_err(|error| generation_error("create wildcard leaf parameters", error))?;

    parameters
        .distinguished_name
        .push(DnType::OrganizationName, "Stackctl Local Development");
    parameters
        .distinguished_name
        .push(DnType::CommonName, "*.stackctl.localhost");
    parameters.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    parameters.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    parameters.use_authority_key_identifier_extension = true;
    parameters.not_before = not_before;
    parameters.not_after = not_after;

    let key = KeyPair::generate().map_err(|error| generation_error("generate leaf key", error))?;
    let certificate = parameters
        .signed_by(&key, issuer)
        .map_err(|error| generation_error("sign wildcard leaf certificate", error))?;

    Ok(LeafCertificateMaterial {
        certificate_pem: certificate.pem(),
        private_key_pem: key.serialize_pem(),
        renew_after,
    })
}
