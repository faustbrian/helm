use crate::control_plane::engine::ContainerCreateOptions;
use crate::control_plane::workload::WorkloadReconcileAction;
use std::collections::BTreeSet;

type ProvisioningIdentity = (String, String, String);

/// Successful project-service provisioning observed by this daemon process.
#[derive(Default)]
pub(crate) struct ProjectServiceProvisioningRegistry {
    completed: BTreeSet<ProvisioningIdentity>,
}

impl ProjectServiceProvisioningRegistry {
    /// Requires an initial/revised job and reruns after service mutation.
    pub(crate) fn requires(
        &self,
        request: &ContainerCreateOptions,
        action: WorkloadReconcileAction,
    ) -> bool {
        action != WorkloadReconcileAction::Unchanged || !self.completed.contains(&identity(request))
    }

    /// Suppresses Engine-event feedback only after the job succeeds.
    pub(crate) fn record(&mut self, request: &ContainerCreateOptions) {
        self.completed.insert(identity(request));
    }
}

fn identity(request: &ContainerCreateOptions) -> ProvisioningIdentity {
    let metadata = request.metadata();

    (
        metadata.project_id().unwrap_or_default().to_owned(),
        metadata.resource_id().unwrap_or_default().to_owned(),
        metadata.desired_revision().to_owned(),
    )
}
