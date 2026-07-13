use time::OffsetDateTime;

/// Stackctl-owned CA and gateway leaf material awaiting secure persistence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LocalCertificateBundle {
    ca_certificate_pem: String,
    ca_private_key_pem: String,
    leaf_certificate_pem: String,
    leaf_private_key_pem: String,
    leaf_renew_after: OffsetDateTime,
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
}
