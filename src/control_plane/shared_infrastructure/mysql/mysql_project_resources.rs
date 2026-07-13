use super::MySqlLogicalResourcePlan;
use crate::control_plane::state::{CredentialRecord, ManagedEnvironmentRecord};

/// Complete durable and executable MySQL-family resources for one project.
#[derive(Debug)]
pub(crate) struct MySqlProjectResources {
    logical: MySqlLogicalResourcePlan,
    credential: CredentialRecord,
    environment: ManagedEnvironmentRecord,
}

impl MySqlProjectResources {
    pub(super) const fn new(
        logical: MySqlLogicalResourcePlan,
        credential: CredentialRecord,
        environment: ManagedEnvironmentRecord,
    ) -> Self {
        Self {
            logical,
            credential,
            environment,
        }
    }

    pub(crate) const fn logical(&self) -> &MySqlLogicalResourcePlan {
        &self.logical
    }

    pub(crate) const fn credential(&self) -> &CredentialRecord {
        &self.credential
    }

    pub(crate) const fn environment(&self) -> &ManagedEnvironmentRecord {
        &self.environment
    }
}
