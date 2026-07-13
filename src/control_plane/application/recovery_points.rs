use super::{ControlPlane, ControlPlaneError};
use crate::control_plane::state::{RecoveryPointRecord, StateStore};

impl<Store> ControlPlane<Store>
where
    Store: StateStore,
{
    /// Publishes immutable recovery evidence after artifact verification.
    pub(crate) fn record_recovery_point(
        &mut self,
        recovery_point: &RecoveryPointRecord,
    ) -> Result<(), ControlPlaneError> {
        self.state_store
            .record_recovery_point(recovery_point)
            .map_err(Into::into)
    }

    /// Loads one project's durable recovery catalog newest-first.
    pub(crate) fn recovery_points(
        &self,
        project_id: &str,
    ) -> Result<Vec<RecoveryPointRecord>, ControlPlaneError> {
        self.state_store
            .recovery_points(project_id)
            .map_err(Into::into)
    }
}
