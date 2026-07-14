use super::ControlPlane;
use crate::control_plane::retention::{InstallationDeletionPlan, InstallationDeletionPlanOptions};
use crate::control_plane::state::StateStore;
use std::collections::BTreeSet;

impl<Store> ControlPlane<Store>
where
    Store: StateStore,
{
    /// Proves every retained logical tenant has exact recovery evidence.
    pub(crate) fn plan_installation_deletion(&self) -> Result<InstallationDeletionPlan, String> {
        let installation = self
            .installation()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "the v8 installation identity has not been initialized".to_owned())?;
        let logical_resources = self
            .logical_resources()
            .map_err(|error| error.to_string())?;
        let credentials = self.credentials().map_err(|error| error.to_string())?;
        let project_ids = logical_resources
            .iter()
            .map(|logical| logical.project_id())
            .collect::<BTreeSet<_>>();
        let mut recovery_points = Vec::new();
        for project_id in project_ids {
            recovery_points.extend(
                self.recovery_points(project_id)
                    .map_err(|error| error.to_string())?,
            );
        }

        InstallationDeletionPlan::new(InstallationDeletionPlanOptions {
            installation_id: installation.installation_id(),
            logical_resources: &logical_resources,
            credentials: &credentials,
            recovery_points: &recovery_points,
        })
    }
}
