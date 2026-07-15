use super::{RetryBackoff, RetryBackoffError, RetryBackoffOptions, RetryDelay};
use crate::control_plane::engine::ContainerCreateOptions;
use crate::control_plane::workload::WorkloadReconcileAction;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::time::{Duration, Instant};

type ProvisioningIdentity = (String, String, String);
const RECHECK_INTERVAL: Duration = Duration::from_secs(15 * 60);
const INITIAL_RETRY_INTERVAL: Duration = Duration::from_millis(500);
const MAXIMUM_RETRY_INTERVAL: Duration = Duration::from_secs(30);

/// Successful project-service provisioning observed by this daemon process.
#[derive(Default)]
pub(crate) struct ProjectServiceProvisioningRegistry {
    completed: BTreeMap<ProvisioningIdentity, Instant>,
    failed: BTreeMap<ProvisioningIdentity, (RetryBackoff, Option<Instant>)>,
}

impl ProjectServiceProvisioningRegistry {
    /// Requires an initial/revised job and reruns after service mutation.
    pub(crate) fn requires(
        &self,
        request: &ContainerCreateOptions,
        action: WorkloadReconcileAction,
        now: Instant,
    ) -> bool {
        let identity = identity(request);

        action != WorkloadReconcileAction::Unchanged
            || self
                .failed
                .get(&identity)
                .is_some_and(|(_, retry_at)| retry_at.is_none_or(|retry_at| now >= retry_at))
            || (self.failed.get(&identity).is_none()
                && self.completed.get(&identity).is_none_or(|completed| {
                    now.saturating_duration_since(*completed) >= RECHECK_INTERVAL
                }))
    }

    /// Suppresses Engine-event feedback only after the job succeeds.
    pub(crate) fn record_success(&mut self, request: &ContainerCreateOptions, now: Instant) {
        let identity = identity(request);
        self.failed.remove(&identity);
        self.completed.insert(identity, now);
    }

    /// Schedules an application-level failure without disconnecting the Engine.
    pub(crate) fn record_failure(
        &mut self,
        request: &ContainerCreateOptions,
        now: Instant,
    ) -> Result<RetryDelay, RetryBackoffError> {
        let identity = identity(request);
        let (retry, retry_at) = match self.failed.entry(identity.clone()) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => entry.insert((
                RetryBackoff::new(
                    format!(
                        "project-service-provisioning:{}/{}/{}",
                        identity.0, identity.1, identity.2
                    ),
                    RetryBackoffOptions::new(INITIAL_RETRY_INTERVAL, MAXIMUM_RETRY_INTERVAL)?,
                )?,
                None,
            )),
        };
        let delay = retry.next_delay();
        self.completed.remove(&identity);
        *retry_at = Some(now + delay.duration());

        Ok(delay)
    }

    /// Activates due retries once so removed services cannot create a loop.
    pub(crate) fn activate_due_retries(&mut self, now: Instant) -> bool {
        let mut activated = false;
        for (_, retry_at) in self.failed.values_mut() {
            if retry_at.is_some_and(|retry_at| now >= retry_at) {
                *retry_at = None;
                activated = true;
            }
        }

        activated
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
