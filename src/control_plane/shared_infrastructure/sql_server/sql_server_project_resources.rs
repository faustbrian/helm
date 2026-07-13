use super::SqlServerLogicalResourcePlan;
use crate::control_plane::state::{CredentialRecord, ManagedEnvironmentRecord};

/// Complete durable SQL Server resources for one project.
#[derive(Debug)]
pub(crate) struct SqlServerProjectResources {
    logical: SqlServerLogicalResourcePlan,
    credential: CredentialRecord,
    environment: ManagedEnvironmentRecord,
}

impl SqlServerProjectResources {
    pub(super) const fn new(
        logical: SqlServerLogicalResourcePlan,
        credential: CredentialRecord,
        environment: ManagedEnvironmentRecord,
    ) -> Self {
        Self {
            logical,
            credential,
            environment,
        }
    }

    pub(crate) const fn logical(&self) -> &SqlServerLogicalResourcePlan {
        &self.logical
    }

    pub(crate) const fn credential(&self) -> &CredentialRecord {
        &self.credential
    }

    pub(crate) const fn environment(&self) -> &ManagedEnvironmentRecord {
        &self.environment
    }
}
