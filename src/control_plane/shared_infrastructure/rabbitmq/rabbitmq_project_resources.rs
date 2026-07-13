use super::RabbitMqProjectDefinition;
use crate::control_plane::state::{CredentialRecord, ManagedEnvironmentRecord};

/// Complete durable RabbitMQ resources for one project.
#[derive(Debug)]
pub(crate) struct RabbitMqProjectResources {
    definition: RabbitMqProjectDefinition,
    credential: CredentialRecord,
    environment: ManagedEnvironmentRecord,
}

impl RabbitMqProjectResources {
    pub(super) const fn new(
        definition: RabbitMqProjectDefinition,
        credential: CredentialRecord,
        environment: ManagedEnvironmentRecord,
    ) -> Self {
        Self {
            definition,
            credential,
            environment,
        }
    }

    pub(crate) const fn definition(&self) -> &RabbitMqProjectDefinition {
        &self.definition
    }

    pub(crate) const fn credential(&self) -> &CredentialRecord {
        &self.credential
    }

    pub(crate) const fn environment(&self) -> &ManagedEnvironmentRecord {
        &self.environment
    }
}
