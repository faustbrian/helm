use super::{LogicalResourceRecordOptions, ResourceLifecycle};

/// Durable project ownership of one tenant inside a shared service instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LogicalResourceRecord {
    options: LogicalResourceRecordOptions,
}

impl LogicalResourceRecord {
    pub(crate) const fn new(options: LogicalResourceRecordOptions) -> Self {
        Self { options }
    }

    pub(crate) fn logical_resource_id(&self) -> &str {
        &self.options.logical_resource_id
    }

    pub(crate) fn shared_resource_id(&self) -> &str {
        &self.options.shared_resource_id
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.options.project_id
    }

    pub(crate) fn service_id(&self) -> &str {
        &self.options.service_id
    }

    pub(crate) fn kind(&self) -> &str {
        &self.options.kind
    }

    pub(crate) fn compatibility_fingerprint(&self) -> &str {
        &self.options.compatibility_fingerprint
    }

    pub(crate) fn desired_revision(&self) -> &str {
        &self.options.desired_revision
    }

    pub(crate) const fn lifecycle(&self) -> ResourceLifecycle {
        self.options.lifecycle
    }

    pub(crate) const fn orphaned_at_unix_seconds(&self) -> Option<i64> {
        self.options.orphaned_at_unix_seconds
    }
}
