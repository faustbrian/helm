use crate::control_plane::state::ManagedEnvironmentRecord;

/// Daemon-owned project environment for one shared Gotenberg endpoint.
#[derive(Debug)]
pub(crate) struct GotenbergProjectResources {
    project_id: String,
    service_id: String,
    environment: ManagedEnvironmentRecord,
}

impl GotenbergProjectResources {
    pub(super) const fn new(
        project_id: String,
        service_id: String,
        environment: ManagedEnvironmentRecord,
    ) -> Self {
        Self {
            project_id,
            service_id,
            environment,
        }
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.project_id
    }

    pub(crate) fn service_id(&self) -> &str {
        &self.service_id
    }

    pub(crate) const fn environment(&self) -> &ManagedEnvironmentRecord {
        &self.environment
    }
}
