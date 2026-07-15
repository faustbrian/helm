use crate::control_plane::engine::ContainerCreateOptions;
use crate::control_plane::workload::WorkloadReconcileAction;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

type ProvisioningIdentity = (String, String, String);
const RECHECK_INTERVAL: Duration = Duration::from_secs(15 * 60);

/// Successful project-service provisioning observed by this daemon process.
#[derive(Default)]
pub(crate) struct ProjectServiceProvisioningRegistry {
    completed: BTreeMap<ProvisioningIdentity, Instant>,
}

impl ProjectServiceProvisioningRegistry {
    /// Requires an initial/revised job and reruns after service mutation.
    pub(crate) fn requires(
        &self,
        request: &ContainerCreateOptions,
        action: WorkloadReconcileAction,
        now: Instant,
    ) -> bool {
        action != WorkloadReconcileAction::Unchanged
            || self
                .completed
                .get(&identity(request))
                .is_none_or(|completed| {
                    now.saturating_duration_since(*completed) >= RECHECK_INTERVAL
                })
    }

    /// Suppresses Engine-event feedback only after the job succeeds.
    pub(crate) fn record(&mut self, request: &ContainerCreateOptions, now: Instant) {
        self.completed.insert(identity(request), now);
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
