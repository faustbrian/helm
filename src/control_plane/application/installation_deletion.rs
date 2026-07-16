use super::ControlPlane;
use crate::control_plane::retention::{
    InstallationDeletionPlan, InstallationDeletionPlanOptions, verify_recovery_point_artifact,
    verify_resource_recovery_point_artifact,
};
use crate::control_plane::state::{
    CredentialRecord, LogicalResourceRecord, RecoveryPointRecord, ResourceRecord, StateStore,
};
use std::collections::BTreeSet;

type InstallationDeletionSnapshot = (
    String,
    Vec<LogicalResourceRecord>,
    Vec<ResourceRecord>,
    Vec<CredentialRecord>,
    Vec<RecoveryPointRecord>,
);

impl<Store> ControlPlane<Store>
where
    Store: StateStore,
{
    /// Proves every retained tenant is recoverable before installation teardown.
    pub(crate) fn plan_installation_deletion(&self) -> Result<InstallationDeletionPlan, String> {
        let (installation_id, logical_resources, resources, credentials, recovery_points) =
            self.installation_deletion_snapshot()?;

        InstallationDeletionPlan::new(InstallationDeletionPlanOptions {
            installation_id: &installation_id,
            logical_resources: &logical_resources,
            resources: &resources,
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
        let active_operations = self
            .state_store
            .active_daemon_operations()
            .map_err(|error| error.to_string())?;
        if !active_operations.is_empty() {
            return Err(format!(
                "installation deletion requires an idle daemon; active operations: {}",
                active_operations
                    .iter()
                    .map(|operation| operation.operation_id())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let (installation_id, logical_resources, resources, _, recovery_points) =
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
        for deletion in plan.volume_deletions() {
            let resource = resources
                .iter()
                .find(|resource| resource.resource_id() == deletion.resource_id())
                .ok_or_else(|| {
                    "installation deletion volume intent changed before freeze".to_owned()
                })?;
            let recovery = recovery_points
                .iter()
                .find(|recovery| recovery.recovery_point_id() == deletion.recovery_point_id())
                .ok_or_else(|| {
                    "installation deletion volume recovery changed before freeze".to_owned()
                })?;
            verify_resource_recovery_point_artifact(recovery, resource, now_unix_seconds)
                .map_err(|error| error.to_string())?;
        }
        self.state_store
            .begin_installation_deletion(now_unix_seconds)
            .map_err(|error| error.to_string())?;

        Ok(plan)
    }

    /// Commits terminal state only after external cleanup has succeeded.
    pub(crate) fn complete_installation_deletion(&mut self) -> Result<(), String> {
        self.state_store
            .complete_installation_deletion()
            .map_err(|error| error.to_string())
    }

    /// Re-verifies exact volume recovery evidence immediately before Engine cleanup.
    pub(crate) fn verified_installation_volume_deletions(
        &self,
        now_unix_seconds: i64,
    ) -> Result<Vec<String>, String> {
        let plan = self.plan_installation_deletion()?;
        let (_, _, resources, _, recovery_points) = self.installation_deletion_snapshot()?;
        let mut authorized = Vec::with_capacity(plan.volume_deletions().len());
        for deletion in plan.volume_deletions() {
            let resource = resources
                .iter()
                .find(|resource| resource.resource_id() == deletion.resource_id())
                .ok_or_else(|| {
                    "installation deletion volume authorization changed before cleanup".to_owned()
                })?;
            let recovery = recovery_points
                .iter()
                .find(|recovery| recovery.recovery_point_id() == deletion.recovery_point_id())
                .ok_or_else(|| {
                    "installation deletion recovery authorization changed before cleanup".to_owned()
                })?;
            verify_resource_recovery_point_artifact(recovery, resource, now_unix_seconds)
                .map_err(|error| error.to_string())?;
            authorized.push(resource.resource_id().to_owned());
        }

        Ok(authorized)
    }

    fn installation_deletion_snapshot(&self) -> Result<InstallationDeletionSnapshot, String> {
        let installation = self
            .installation()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "the v8 installation identity has not been initialized".to_owned())?;
        let logical_resources = self
            .logical_resources()
            .map_err(|error| error.to_string())?;
        let resources = self.resources().map_err(|error| error.to_string())?;
        let credentials = self.credentials().map_err(|error| error.to_string())?;
        let project_ids = logical_resources
            .iter()
            .map(|logical| logical.project_id())
            .chain(resources.iter().filter_map(ResourceRecord::project_id))
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
            resources,
            credentials,
            recovery_points,
        ))
    }
}
