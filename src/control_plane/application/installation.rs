use super::{ControlPlane, ControlPlaneError};
use crate::control_plane::state::{InstallationRecord, StateStore};

impl<Store> ControlPlane<Store>
where
    Store: StateStore,
{
    /// Loads immutable per-user installation identity and Engine selection.
    pub(crate) fn installation(&self) -> Result<Option<InstallationRecord>, ControlPlaneError> {
        self.state_store.installation().map_err(Into::into)
    }
}
