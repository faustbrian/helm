use super::LocalCertificateError;
use std::fmt::{Debug, Formatter};
use time::OffsetDateTime;
use x509_parser::parse_x509_certificate;
use x509_parser::pem::parse_x509_pem;

/// Stackctl-owned CA and gateway leaf material awaiting secure persistence.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct LocalCertificateBundle {
    ca_certificate_pem: String,
    ca_private_key_pem: String,
    leaf_certificate_pem: String,
    leaf_private_key_pem: String,
    leaf_renew_after: OffsetDateTime,
}

impl Debug for LocalCertificateBundle {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LocalCertificateBundle")
            .field("ca_certificate_pem", &self.ca_certificate_pem)
            .field("ca_private_key_pem", &"[REDACTED]")
            .field("leaf_certificate_pem", &self.leaf_certificate_pem)
            .field("leaf_private_key_pem", &"[REDACTED]")
            .field("leaf_renew_after", &self.leaf_renew_after)
            .finish()
    }
}

impl LocalCertificateBundle {
    pub(super) fn new(
        ca_certificate_pem: String,
        ca_private_key_pem: String,
        leaf_certificate_pem: String,
        leaf_private_key_pem: String,
        leaf_renew_after: OffsetDateTime,
    ) -> Self {
        Self {
            ca_certificate_pem,
            ca_private_key_pem,
            leaf_certificate_pem,
            leaf_private_key_pem,
            leaf_renew_after,
        }
    }

    pub(crate) fn ca_certificate_pem(&self) -> &str {
        &self.ca_certificate_pem
    }

    pub(crate) fn ca_private_key_pem(&self) -> &str {
        &self.ca_private_key_pem
    }

    pub(crate) fn leaf_certificate_pem(&self) -> &str {
        &self.leaf_certificate_pem
    }

    pub(crate) fn leaf_private_key_pem(&self) -> &str {
        &self.leaf_private_key_pem
    }

    pub(crate) const fn leaf_renew_after(&self) -> OffsetDateTime {
        self.leaf_renew_after
    }

    pub(crate) fn leaf_expires_at(&self) -> Result<OffsetDateTime, LocalCertificateError> {
        let (_, pem) = parse_x509_pem(self.leaf_certificate_pem.as_bytes()).map_err(|error| {
            LocalCertificateError::new(format!(
                "gateway leaf certificate is not valid PEM: {error}"
            ))
        })?;
        let (_, certificate) = parse_x509_certificate(&pem.contents).map_err(|error| {
            LocalCertificateError::new(format!(
                "gateway leaf certificate is not valid X.509: {error}"
            ))
        })?;

        OffsetDateTime::from_unix_timestamp(certificate.validity().not_after.timestamp()).map_err(
            |error| {
                LocalCertificateError::new(format!(
                    "gateway leaf certificate expiry is outside the supported range: {error}"
                ))
            },
        )
    }
}
