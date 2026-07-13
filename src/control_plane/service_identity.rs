use super::{DnsLabel, IdentityError};

/// The exact validated identity of a v8 service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ServiceIdentity(DnsLabel);

impl ServiceIdentity {
    /// Validates a service name without changing it.
    pub(crate) fn new(name: &str) -> Result<Self, IdentityError> {
        DnsLabel::new("service", name).map(Self)
    }

    /// Returns the exact service name supplied by configuration.
    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }
}
