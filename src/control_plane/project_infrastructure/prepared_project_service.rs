use super::{ProjectServiceContainerConfiguration, ProjectServicePreparationError};
use crate::control_plane::engine::BindMount;
use crate::control_plane::gateway::GatewayRoute;
use crate::control_plane::state::{CredentialRecord, ManagedEnvironmentRecord};
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};

/// Stable generated state required by one prepared project service.
#[derive(Eq, PartialEq)]
pub(crate) struct PreparedProjectService {
    project_id: String,
    service_id: String,
    credential: Option<CredentialRecord>,
    environment: ManagedEnvironmentRecord,
    container_environment: BTreeMap<String, String>,
    container_command: Option<Vec<String>>,
    container_configuration: Option<ProjectServiceContainerConfiguration>,
    container_configuration_mount: Option<BindMount>,
    route: Option<GatewayRoute>,
}

impl PreparedProjectService {
    pub(super) const fn new(
        project_id: String,
        service_id: String,
        credential: Option<CredentialRecord>,
        environment: ManagedEnvironmentRecord,
        container_environment: BTreeMap<String, String>,
        route: Option<GatewayRoute>,
    ) -> Self {
        Self {
            project_id,
            service_id,
            credential,
            environment,
            container_environment,
            container_command: None,
            container_configuration: None,
            container_configuration_mount: None,
            route,
        }
    }

    pub(crate) fn with_container_command(
        mut self,
        command: Vec<String>,
    ) -> Result<Self, ProjectServicePreparationError> {
        if command.is_empty() || command.iter().any(String::is_empty) {
            return Err(ProjectServicePreparationError::new(
                "generated project service command must contain non-empty arguments".to_owned(),
            ));
        }
        self.container_command = Some(command);

        Ok(self)
    }

    pub(crate) fn with_container_configuration(
        mut self,
        configuration: ProjectServiceContainerConfiguration,
    ) -> Self {
        self.container_configuration = Some(configuration);

        self
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.project_id
    }

    pub(crate) fn service_id(&self) -> &str {
        &self.service_id
    }

    pub(crate) const fn credential(&self) -> Option<&CredentialRecord> {
        self.credential.as_ref()
    }

    pub(crate) const fn environment(&self) -> &ManagedEnvironmentRecord {
        &self.environment
    }

    pub(crate) const fn container_environment(&self) -> &BTreeMap<String, String> {
        &self.container_environment
    }

    pub(crate) fn container_command(&self) -> Option<&[String]> {
        self.container_command.as_deref()
    }

    pub(crate) const fn container_configuration(
        &self,
    ) -> Option<&ProjectServiceContainerConfiguration> {
        self.container_configuration.as_ref()
    }

    pub(crate) const fn container_configuration_mount(&self) -> Option<&BindMount> {
        self.container_configuration_mount.as_ref()
    }

    pub(super) fn set_container_configuration_mount(&mut self, mount: BindMount) {
        self.container_configuration_mount = Some(mount);
    }

    pub(crate) const fn route(&self) -> Option<&GatewayRoute> {
        self.route.as_ref()
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
            .field("container_command", &self.container_command())
            .field("container_configuration", &self.container_configuration())
            .field(
                "container_configuration_mount",
                &self.container_configuration_mount(),
            )
            .field("route", &self.route())
            .finish()
    }
}
