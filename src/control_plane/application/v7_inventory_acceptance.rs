use super::{ControlPlane, ControlPlaneError};
use crate::control_plane::state::{AcceptedV7InventoryRecord, StateStore};
use std::path::Path;

impl<Store> ControlPlane<Store>
where
    Store: StateStore,
{
    /// Appends one explicitly accepted immutable legacy source observation.
    pub(crate) fn record_accepted_v7_inventory(
        &mut self,
        inventory: &AcceptedV7InventoryRecord,
    ) -> Result<(), ControlPlaneError> {
        self.state_store
            .record_accepted_v7_inventory(inventory)
            .map_err(Into::into)
    }

    /// Loads exact accepted evidence for one project and observation digest.
    pub(crate) fn accepted_v7_inventory(
        &self,
        canonical_project_path: &Path,
        evidence_revision: &str,
    ) -> Result<Option<AcceptedV7InventoryRecord>, ControlPlaneError> {
        self.state_store
            .accepted_v7_inventory(canonical_project_path, evidence_revision)
            .map_err(Into::into)
    }

    /// Loads the newest explicitly accepted observation for one project path.
    pub(crate) fn latest_accepted_v7_inventory(
        &self,
        canonical_project_path: &Path,
    ) -> Result<Option<AcceptedV7InventoryRecord>, ControlPlaneError> {
        self.state_store
            .latest_accepted_v7_inventory(canonical_project_path)
            .map_err(Into::into)
    }
}
