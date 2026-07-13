use super::ObjectStoreProjectDefinition;
use crate::control_plane::state::{CredentialRecord, ManagedEnvironmentRecord};

/// Complete durable object-store resources for one project service.
#[derive(Debug)]
pub(crate) struct ObjectStoreProjectResources {
    definition: ObjectStoreProjectDefinition,
    credential: CredentialRecord,
    environment: ManagedEnvironmentRecord,
}

impl ObjectStoreProjectResources {
    pub(super) const fn new(
        definition: ObjectStoreProjectDefinition,
        credential: CredentialRecord,
        environment: ManagedEnvironmentRecord,
    ) -> Self {
        Self {
            definition,
            credential,
            environment,
        }
    }

    pub(crate) const fn definition(&self) -> &ObjectStoreProjectDefinition {
        &self.definition
    }

    pub(crate) const fn credential(&self) -> &CredentialRecord {
        &self.credential
    }

    pub(crate) const fn environment(&self) -> &ManagedEnvironmentRecord {
        &self.environment
    }
}
