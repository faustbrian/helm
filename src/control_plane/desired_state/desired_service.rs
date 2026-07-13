use crate::control_plane::ServiceIdentity;

/// One service after identity and dependency validation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DesiredService {
    identity: ServiceIdentity,
    dependencies: Vec<ServiceIdentity>,
}

impl DesiredService {
    pub(super) fn new(identity: ServiceIdentity, dependencies: Vec<ServiceIdentity>) -> Self {
        Self {
            identity,
            dependencies,
        }
    }

    /// Returns the exact validated service identity.
    pub(crate) fn name(&self) -> &str {
        self.identity.as_str()
    }

    /// Returns dependencies in deterministic identity order.
    pub(crate) fn dependencies(&self) -> &[ServiceIdentity] {
        &self.dependencies
    }

    pub(super) const fn identity(&self) -> &ServiceIdentity {
        &self.identity
    }
}
