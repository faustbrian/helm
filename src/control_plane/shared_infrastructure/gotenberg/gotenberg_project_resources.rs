use crate::control_plane::state::ManagedEnvironmentRecord;

/// Daemon-owned project environment for one shared Gotenberg endpoint.
#[derive(Debug)]
pub(crate) struct GotenbergProjectResources {
    environment: ManagedEnvironmentRecord,
}

impl GotenbergProjectResources {
    pub(super) const fn new(environment: ManagedEnvironmentRecord) -> Self {
        Self { environment }
    }

    pub(crate) const fn environment(&self) -> &ManagedEnvironmentRecord {
        &self.environment
    }
}
