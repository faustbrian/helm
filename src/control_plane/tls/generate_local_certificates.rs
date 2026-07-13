use super::{LocalCertificateBundle, LocalCertificateError};
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use time::{Duration, OffsetDateTime};

const CA_VALIDITY_DAYS: i64 = 3_650;
const LEAF_VALIDITY_DAYS: i64 = 90;
const LEAF_RENEWAL_DAYS: i64 = 75;

/// Generates one Stackctl CA and its renewable wildcard gateway certificate.
pub(crate) fn generate_local_certificates(
    now: OffsetDateTime,
) -> Result<LocalCertificateBundle, LocalCertificateError> {
    let not_before = checked_time(now, -1, "certificate not-before")?;
    let ca_not_after = checked_time(now, CA_VALIDITY_DAYS, "CA expiry")?;
    let leaf_not_after = checked_time(now, LEAF_VALIDITY_DAYS, "leaf expiry")?;
    let leaf_renew_after = checked_time(now, LEAF_RENEWAL_DAYS, "leaf renewal")?;

    let mut ca_params = CertificateParams::new(Vec::<String>::new())
        .map_err(|error| generation_error("create CA parameters", error))?;
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params
        .distinguished_name
        .push(DnType::OrganizationName, "Stackctl Local Development");
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "Stackctl Local CA");
    ca_params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
    ];
    ca_params.not_before = not_before;
    ca_params.not_after = ca_not_after;

    let ca_key = KeyPair::generate().map_err(|error| generation_error("generate CA key", error))?;
    let ca_certificate = ca_params
        .self_signed(&ca_key)
        .map_err(|error| generation_error("self-sign CA certificate", error))?;
    let issuer = Issuer::from_params(&ca_params, &ca_key);

    let mut leaf_params = CertificateParams::new(vec!["*.stackctl.localhost".to_owned()])
        .map_err(|error| generation_error("create wildcard leaf parameters", error))?;
    leaf_params
        .distinguished_name
        .push(DnType::OrganizationName, "Stackctl Local Development");
    leaf_params
        .distinguished_name
        .push(DnType::CommonName, "*.stackctl.localhost");
    leaf_params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    leaf_params.use_authority_key_identifier_extension = true;
    leaf_params.not_before = not_before;
    leaf_params.not_after = leaf_not_after;

    let leaf_key =
        KeyPair::generate().map_err(|error| generation_error("generate leaf key", error))?;
    let leaf_certificate = leaf_params
        .signed_by(&leaf_key, &issuer)
        .map_err(|error| generation_error("sign wildcard leaf certificate", error))?;

    Ok(LocalCertificateBundle::new(
        ca_certificate.pem(),
        ca_key.serialize_pem(),
        leaf_certificate.pem(),
        leaf_key.serialize_pem(),
        leaf_renew_after,
    ))
}

fn checked_time(
    now: OffsetDateTime,
    days: i64,
    field: &str,
) -> Result<OffsetDateTime, LocalCertificateError> {
    now.checked_add(Duration::days(days))
        .ok_or_else(|| LocalCertificateError::new(format!("{field} is outside supported time")))
}

fn generation_error(action: &str, error: rcgen::Error) -> LocalCertificateError {
    LocalCertificateError::new(format!("failed to {action}: {error}"))
}
