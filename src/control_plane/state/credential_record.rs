use super::{CredentialLifecycle, CredentialRecordOptions};
use std::fmt::{Debug, Formatter};

/// Durable stable credential owned by one project service.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct CredentialRecord {
    options: CredentialRecordOptions,
}

impl CredentialRecord {
    pub(crate) const fn new(options: CredentialRecordOptions) -> Self {
        Self { options }
    }

    pub(crate) fn credential_id(&self) -> &str {
        &self.options.credential_id
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.options.project_id
    }

    pub(crate) fn service_id(&self) -> &str {
        &self.options.service_id
    }

    pub(crate) fn username(&self) -> &str {
        &self.options.username
    }

    pub(crate) fn secret(&self) -> &str {
        &self.options.secret
    }

    pub(crate) const fn lifecycle(&self) -> CredentialLifecycle {
        self.options.lifecycle
    }
}

impl Debug for CredentialRecord {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CredentialRecord")
            .field("credential_id", &self.credential_id())
            .field("project_id", &self.project_id())
            .field("service_id", &self.service_id())
            .field("username", &self.username())
            .field("secret", &"[REDACTED]")
            .field("lifecycle", &self.lifecycle())
            .finish()
    }
}
