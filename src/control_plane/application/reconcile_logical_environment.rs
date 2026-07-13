use super::{ControlPlane, ControlPlaneError};
use crate::control_plane::state::{LogicalResourceRecord, ManagedEnvironmentRecord, StateStore};

impl<Store> ControlPlane<Store>
where
    Store: StateStore,
{
    /// Publishes one complete project logical-service set atomically.
    pub(crate) fn reconcile_logical_environment(
        &mut self,
        resources: &[LogicalResourceRecord],
        environment: &ManagedEnvironmentRecord,
        orphaned_at_unix_seconds: i64,
    ) -> Result<(), ControlPlaneError> {
        self.state_store
            .reconcile_logical_environment(resources, environment, orphaned_at_unix_seconds)
            .map_err(Into::into)
    }
}
