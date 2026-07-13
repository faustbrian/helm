use super::{ControlPlane, ControlPlaneError};
use crate::control_plane::state::{LogicalResourceRecord, StateStore};

impl<Store> ControlPlane<Store>
where
    Store: StateStore,
{
    /// Loads durable logical ownership for shared-service lifecycle decisions.
    pub(crate) fn logical_resources(
        &self,
    ) -> Result<Vec<LogicalResourceRecord>, ControlPlaneError> {
        self.state_store.logical_resources().map_err(Into::into)
    }
}
