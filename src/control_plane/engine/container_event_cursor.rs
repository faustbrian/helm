use super::{ContainerEvent, ContainerEventAction};

/// Last processed Engine event timestamp used for reconnect deduplication.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ContainerEventCursor {
    nanoseconds: u64,
    container_id: Option<String>,
    action: Option<ContainerEventAction>,
}

impl ContainerEventCursor {
    pub(crate) const fn beginning() -> Self {
        Self {
            nanoseconds: 0,
            container_id: None,
            action: None,
        }
    }

    pub(crate) const fn nanoseconds(&self) -> u64 {
        self.nanoseconds
    }

    pub(crate) fn advance(mut self, event: &ContainerEvent) -> Self {
        if event.occurred_at_nanoseconds() >= self.nanoseconds {
            self.nanoseconds = event.occurred_at_nanoseconds();
            self.container_id = Some(event.container_id().as_str().to_owned());
            self.action = Some(event.action().clone());
        }

        self
    }

    pub(super) fn since_seconds(&self) -> Option<String> {
        (self.nanoseconds > 0).then(|| (self.nanoseconds / 1_000_000_000).to_string())
    }

    pub(super) fn has_processed(
        &self,
        container_id: &str,
        action: &ContainerEventAction,
        occurred_at_nanoseconds: u64,
    ) -> bool {
        occurred_at_nanoseconds < self.nanoseconds
            || (occurred_at_nanoseconds == self.nanoseconds
                && self.container_id.as_deref() == Some(container_id)
                && self.action.as_ref() == Some(action))
    }
}
