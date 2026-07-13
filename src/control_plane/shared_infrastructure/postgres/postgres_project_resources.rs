use super::PostgresLogicalResourcePlan;
use crate::control_plane::state::{CredentialRecord, ManagedEnvironmentRecord};

/// Complete durable and executable PostgreSQL resources for one project.
#[derive(Debug)]
pub(crate) struct PostgresProjectResources {
    logical: PostgresLogicalResourcePlan,
    credential: CredentialRecord,
    environment: ManagedEnvironmentRecord,
}

impl PostgresProjectResources {
    pub(super) const fn new(
        logical: PostgresLogicalResourcePlan,
        credential: CredentialRecord,
        environment: ManagedEnvironmentRecord,
    ) -> Self {
        Self {
            logical,
            credential,
            environment,
        }
    }

    pub(crate) const fn logical(&self) -> &PostgresLogicalResourcePlan {
        &self.logical
    }

    pub(crate) const fn credential(&self) -> &CredentialRecord {
        &self.credential
    }

    pub(crate) const fn environment(&self) -> &ManagedEnvironmentRecord {
        &self.environment
    }
}
