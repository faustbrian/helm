use super::ControlPlane;
use crate::control_plane::retention::{
    InstallationDeletionPlan, InstallationDeletionPlanOptions, verify_recovery_point_artifact,
};
use crate::control_plane::state::{
    CredentialRecord, LogicalResourceRecord, RecoveryPointRecord, StateStore,
};
use std::collections::BTreeSet;

impl<Store> ControlPlane<Store>
where
    Store: StateStore,
{
    /// Proves every retained logical tenant has exact recovery evidence.
    pub(crate) fn plan_installation_deletion(&self) -> Result<InstallationDeletionPlan, String> {
        let (installation_id, logical_resources, credentials, recovery_points) =
            self.installation_deletion_snapshot()?;

        InstallationDeletionPlan::new(InstallationDeletionPlanOptions {
            installation_id: &installation_id,
            logical_resources: &logical_resources,
            credentials: &credentials,
            recovery_points: &recovery_points,
        })
    }

    /// Revalidates complete intent and artifacts before atomically freezing state.
    pub(crate) fn begin_confirmed_installation_deletion(
        &mut self,
        confirmation_token: &str,
        now_unix_seconds: i64,
    ) -> Result<InstallationDeletionPlan, String> {
        let plan = self.plan_installation_deletion()?;
        if plan.confirmation_token() != confirmation_token {
            return Err("installation deletion confirmation token is stale".to_owned());
        }
        let (installation_id, logical_resources, _, recovery_points) =
            self.installation_deletion_snapshot()?;
        for prune in plan.logical_prunes() {
            let logical = logical_resources
                .iter()
                .find(|logical| {
                    logical.logical_resource_id() == prune.logical_resource_id()
                        && logical.project_id() == prune.project_id()
                        && logical.service_id() == prune.service_id()
                })
                .ok_or_else(|| {
                    "installation deletion logical intent changed before freeze".to_owned()
                })?;
            let recovery = recovery_points
                .iter()
                .find(|recovery| recovery.recovery_point_id() == prune.recovery_point_id())
                .ok_or_else(|| {
                    "installation deletion recovery intent changed before freeze".to_owned()
                })?;
            verify_recovery_point_artifact(recovery, logical, &installation_id, now_unix_seconds)
                .map_err(|error| error.to_string())?;
        }
        self.state_store
            .begin_installation_deletion(now_unix_seconds)
            .map_err(|error| error.to_string())?;

        Ok(plan)
    }

    fn installation_deletion_snapshot(
        &self,
    ) -> Result<
        (
            String,
            Vec<LogicalResourceRecord>,
            Vec<CredentialRecord>,
            Vec<RecoveryPointRecord>,
        ),
        String,
    > {
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

        Ok((
            installation.installation_id().to_owned(),
            logical_resources,
            credentials,
            recovery_points,
        ))
    }
}
