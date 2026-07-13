use super::LocalCertificateError;
use sha1::{Digest as Sha1Digest, Sha1};
use sha2::{Digest as Sha256Digest, Sha256};
use x509_parser::{parse_x509_certificate, pem::parse_x509_pem};

/// Exact SHA-256 identity of one validated CA certificate.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct LocalCaIdentity {
    sha1_hex: String,
    sha256_hex: String,
}

impl LocalCaIdentity {
    pub(crate) fn from_pem(pem: &str) -> Result<Self, LocalCertificateError> {
        let (_, pem) = parse_x509_pem(pem.as_bytes()).map_err(|error| {
            LocalCertificateError::new(format!("failed to parse CA certificate PEM: {error}"))
        })?;
        let (_, certificate) = parse_x509_certificate(&pem.contents).map_err(|error| {
            LocalCertificateError::new(format!("failed to parse CA certificate DER: {error}"))
        })?;

        if !certificate.is_ca() {
            return Err(LocalCertificateError::new("certificate is not a CA"));
        }

        Ok(Self {
            sha1_hex: hex::encode_upper(Sha1::digest(&pem.contents)),
            sha256_hex: hex::encode_upper(Sha256::digest(&pem.contents)),
        })
    }

    pub(crate) fn sha1_hex(&self) -> &str {
        &self.sha1_hex
    }

    pub(crate) fn sha256_hex(&self) -> &str {
        &self.sha256_hex
    }
}
