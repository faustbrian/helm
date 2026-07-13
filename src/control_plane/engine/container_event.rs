use super::{ContainerEventAction, ContainerId};

/// Backend-independent managed-container event consumed by reconciliation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ContainerEvent {
    container_id: ContainerId,
    action: ContainerEventAction,
    occurred_at_nanoseconds: u64,
}

impl ContainerEvent {
    pub(crate) const fn new(
        container_id: ContainerId,
        action: ContainerEventAction,
        occurred_at_nanoseconds: u64,
    ) -> Self {
        Self {
            container_id,
            action,
            occurred_at_nanoseconds,
        }
    }

    pub(crate) const fn container_id(&self) -> &ContainerId {
        &self.container_id
    }

    pub(crate) const fn action(&self) -> &ContainerEventAction {
        &self.action
    }

    pub(crate) const fn occurred_at_nanoseconds(&self) -> u64 {
        self.occurred_at_nanoseconds
    }
}
