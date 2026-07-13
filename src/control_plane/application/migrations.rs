use super::{ControlPlane, ControlPlaneError};
use crate::control_plane::state::{MigrationRecord, StateStore};

impl<Store> ControlPlane<Store>
where
    Store: StateStore,
{
    /// Loads durable reversible-migration checkpoints in stable identity order.
    pub(crate) fn migrations(&self) -> Result<Vec<MigrationRecord>, ControlPlaneError> {
        self.state_store.migrations().map_err(Into::into)
    }
}
