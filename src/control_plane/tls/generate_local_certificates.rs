use super::{LocalCertificateBundle, LocalCertificateError, generate_leaf_certificate};
use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair, KeyUsagePurpose};
use time::{Duration, OffsetDateTime};

const CA_VALIDITY_DAYS: i64 = 3_650;

/// Generates one Stackctl CA and its renewable wildcard gateway certificate.
pub(crate) fn generate_local_certificates(
    now: OffsetDateTime,
) -> Result<LocalCertificateBundle, LocalCertificateError> {
    let not_before = checked_time(now, -1, "certificate not-before")?;
    let ca_not_after = checked_time(now, CA_VALIDITY_DAYS, "CA expiry")?;

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

    let leaf = generate_leaf_certificate(&issuer, now)?;

    Ok(LocalCertificateBundle::new(
        ca_certificate.pem(),
        ca_key.serialize_pem(),
        leaf.certificate_pem,
        leaf.private_key_pem,
        leaf.renew_after,
    ))
}

pub(super) fn checked_time(
    now: OffsetDateTime,
    days: i64,
    field: &str,
) -> Result<OffsetDateTime, LocalCertificateError> {
    now.checked_add(Duration::days(days))
        .ok_or_else(|| LocalCertificateError::new(format!("{field} is outside supported time")))
}

pub(super) fn generation_error(action: &str, error: rcgen::Error) -> LocalCertificateError {
    LocalCertificateError::new(format!("failed to {action}: {error}"))
}
