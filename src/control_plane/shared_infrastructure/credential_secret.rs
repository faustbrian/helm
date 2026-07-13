use std::fmt::{Debug, Formatter};

/// Redaction-safe 256-bit project credential encoded for service transport.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct CredentialSecret {
    encoded: String,
}

impl CredentialSecret {
    pub(super) const fn new(encoded: String) -> Self {
        Self { encoded }
    }

    pub(crate) fn expose(&self) -> &str {
        &self.encoded
    }
}

impl Debug for CredentialSecret {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("CredentialSecret([REDACTED])")
    }
}
