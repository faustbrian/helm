use super::MigrationOperationError;
use crate::control_plane::state::{
    CredentialLifecycle, CredentialRecord, LogicalResourceRecord, ResourceLifecycle,
};

/// Exact durable ownership returned after idempotent target provisioning.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MigrationTargetPlan {
    target_resource_id: String,
    logical_resource: LogicalResourceRecord,
    credential: CredentialRecord,
}

impl MigrationTargetPlan {
    pub(crate) fn new(
        target_resource_id: impl Into<String>,
        logical_resource: LogicalResourceRecord,
        credential: CredentialRecord,
    ) -> Result<Self, MigrationOperationError> {
        let target_resource_id = target_resource_id.into();
        if target_resource_id.is_empty()
            || logical_resource.lifecycle() != ResourceLifecycle::Active
            || credential.lifecycle() != CredentialLifecycle::Active
            || credential.project_id() != Some(logical_resource.project_id())
            || credential.service_id() != logical_resource.service_id()
        {
            return Err(MigrationOperationError::new(
                "provisioned target returned incomplete durable ownership",
            ));
        }

        Ok(Self {
            target_resource_id,
            logical_resource,
            credential,
        })
    }

    pub(crate) fn target_resource_id(&self) -> &str {
        &self.target_resource_id
    }

    pub(crate) const fn logical_resource(&self) -> &LogicalResourceRecord {
        &self.logical_resource
    }

    pub(crate) const fn credential(&self) -> &CredentialRecord {
        &self.credential
    }
}
