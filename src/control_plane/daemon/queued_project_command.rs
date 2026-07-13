use super::PersistedProjectCommand;
use crate::control_plane::workload::ProjectCommandPlan;
use std::fmt::{Debug, Formatter};

/// One validated project command waiting for singleton Engine execution.
pub(crate) struct QueuedProjectCommand {
    operation_id: String,
    service_id: String,
    plan: ProjectCommandPlan,
}

impl QueuedProjectCommand {
    pub(crate) fn new(operation_id: String, service_id: String, plan: ProjectCommandPlan) -> Self {
        Self {
            operation_id,
            service_id,
            plan,
        }
    }

    pub(crate) fn operation_id(&self) -> &str {
        &self.operation_id
    }

    pub(crate) const fn plan(&self) -> &ProjectCommandPlan {
        &self.plan
    }

    pub(crate) fn service_id(&self) -> &str {
        &self.service_id
    }

    pub(crate) fn payload_json(&self) -> Result<String, String> {
        let persisted = PersistedProjectCommand::from_queued(self)?;
        serde_json::to_string(&persisted)
            .map_err(|error| format!("failed to encode project command: {error}"))
    }

    pub(crate) fn from_payload_json(
        operation_id: String,
        payload_json: &str,
    ) -> Result<Self, String> {
        let persisted = serde_json::from_str::<PersistedProjectCommand>(payload_json)
            .map_err(|error| format!("failed to decode project command: {error}"))?;

        persisted.into_queued(operation_id)
    }
}

impl Debug for QueuedProjectCommand {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("QueuedProjectCommand")
            .field("operation_id", &self.operation_id)
            .field("service_id", &self.service_id)
            .field("plan", &self.plan)
            .finish()
    }
}
