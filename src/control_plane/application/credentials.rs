use super::{ControlPlane, ControlPlaneError};
use crate::control_plane::state::{CredentialRecord, StateStore};

impl<Store> ControlPlane<Store>
where
    Store: StateStore,
{
    /// Loads retained credentials for runtime-only operation resolution.
    pub(crate) fn credentials(&self) -> Result<Vec<CredentialRecord>, ControlPlaneError> {
        self.state_store.credentials().map_err(Into::into)
    }
}
