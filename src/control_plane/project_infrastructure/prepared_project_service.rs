use crate::control_plane::gateway::GatewayRoute;
use crate::control_plane::state::{CredentialRecord, ManagedEnvironmentRecord};
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};

/// Stable generated state required by one routable project service.
#[derive(Eq, PartialEq)]
pub(crate) struct PreparedProjectService {
    project_id: String,
    service_id: String,
    credential: CredentialRecord,
    environment: ManagedEnvironmentRecord,
    container_environment: BTreeMap<String, String>,
    route: GatewayRoute,
}

impl PreparedProjectService {
    pub(super) const fn new(
        project_id: String,
        service_id: String,
        credential: CredentialRecord,
        environment: ManagedEnvironmentRecord,
        container_environment: BTreeMap<String, String>,
        route: GatewayRoute,
    ) -> Self {
        Self {
            project_id,
            service_id,
            credential,
            environment,
            container_environment,
            route,
        }
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.project_id
    }

    pub(crate) fn service_id(&self) -> &str {
        &self.service_id
    }

    pub(crate) const fn credential(&self) -> &CredentialRecord {
        &self.credential
    }

    pub(crate) const fn environment(&self) -> &ManagedEnvironmentRecord {
        &self.environment
    }

    pub(crate) const fn container_environment(&self) -> &BTreeMap<String, String> {
        &self.container_environment
    }

    pub(crate) const fn route(&self) -> &GatewayRoute {
        &self.route
    }
}

impl Debug for PreparedProjectService {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedProjectService")
            .field("project_id", &self.project_id())
            .field("service_id", &self.service_id())
            .field("credential", &self.credential())
            .field("environment", &self.environment())
            .field(
                "container_environment_keys",
                &self.container_environment().keys(),
            )
            .field("route", &self.route())
            .finish()
    }
}
