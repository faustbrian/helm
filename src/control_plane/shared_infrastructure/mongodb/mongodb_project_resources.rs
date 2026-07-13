use super::MongoDbLogicalResourcePlan;
use crate::control_plane::state::{CredentialRecord, ManagedEnvironmentRecord};

/// Complete durable MongoDB resources for one project.
#[derive(Debug)]
pub(crate) struct MongoDbProjectResources {
    logical: MongoDbLogicalResourcePlan,
    credential: CredentialRecord,
    environment: ManagedEnvironmentRecord,
}

impl MongoDbProjectResources {
    pub(super) const fn new(
        logical: MongoDbLogicalResourcePlan,
        credential: CredentialRecord,
        environment: ManagedEnvironmentRecord,
    ) -> Self {
        Self {
            logical,
            credential,
            environment,
        }
    }

    pub(crate) const fn logical(&self) -> &MongoDbLogicalResourcePlan {
        &self.logical
    }

    pub(crate) const fn credential(&self) -> &CredentialRecord {
        &self.credential
    }

    pub(crate) const fn environment(&self) -> &ManagedEnvironmentRecord {
        &self.environment
    }
}
