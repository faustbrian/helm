use super::MailpitProjectDefinition;
use crate::control_plane::gateway::GatewayRoute;
use crate::control_plane::state::{CredentialRecord, ManagedEnvironmentRecord};

/// Complete attributed mail resources for one project.
#[derive(Debug)]
pub(crate) struct MailpitProjectResources {
    definition: MailpitProjectDefinition,
    credential: CredentialRecord,
    environment: ManagedEnvironmentRecord,
    route: GatewayRoute,
}

impl MailpitProjectResources {
    pub(super) const fn new(
        definition: MailpitProjectDefinition,
        credential: CredentialRecord,
        environment: ManagedEnvironmentRecord,
        route: GatewayRoute,
    ) -> Self {
        Self {
            definition,
            credential,
            environment,
            route,
        }
    }

    pub(crate) const fn definition(&self) -> &MailpitProjectDefinition {
        &self.definition
    }

    pub(crate) const fn credential(&self) -> &CredentialRecord {
        &self.credential
    }

    pub(crate) const fn environment(&self) -> &ManagedEnvironmentRecord {
        &self.environment
    }

    pub(crate) const fn route(&self) -> &GatewayRoute {
        &self.route
    }
}
