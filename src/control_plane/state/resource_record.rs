use super::{ResourceLifecycle, ResourceRecordOptions, ResourceRetention};

/// Durable proof and lifecycle state for one Stackctl-managed Engine object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResourceRecord {
    options: ResourceRecordOptions,
    scope_id: Option<String>,
}

impl ResourceRecord {
    pub(crate) fn new(options: ResourceRecordOptions) -> Self {
        Self {
            options,
            scope_id: None,
        }
    }

    /// Assigns the stable desired-resource slot represented by a backend ID.
    pub(crate) fn with_scope_id(mut self, scope_id: impl Into<String>) -> Self {
        self.scope_id = Some(scope_id.into());

        self
    }

    pub(crate) fn resource_id(&self) -> &str {
        &self.options.resource_id
    }

    pub(crate) fn scope_id(&self) -> Option<&str> {
        self.scope_id.as_deref()
    }

    pub(crate) fn installation_id(&self) -> &str {
        &self.options.installation_id
    }

    pub(crate) fn kind(&self) -> &str {
        &self.options.kind
    }

    pub(crate) fn compatibility_fingerprint(&self) -> &str {
        &self.options.compatibility_fingerprint
    }

    pub(crate) fn project_id(&self) -> Option<&str> {
        self.options.project_id.as_deref()
    }

    pub(crate) const fn schema_version(&self) -> u32 {
        self.options.schema_version
    }

    pub(crate) fn desired_revision(&self) -> &str {
        &self.options.desired_revision
    }

    pub(crate) const fn retention(&self) -> ResourceRetention {
        self.options.retention
    }

    pub(crate) const fn lifecycle(&self) -> ResourceLifecycle {
        self.options.lifecycle
    }

    pub(crate) const fn orphaned_at_unix_seconds(&self) -> Option<i64> {
        self.options.orphaned_at_unix_seconds
    }
}
