use super::ipc::IpcMigrationDecision;
use serde::{Deserialize, Serialize};

/// Secret-free identity of one explicit migration decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QueuedMigrationDecision {
    operation_id: String,
    migration_id: String,
    project_id: String,
    decision: IpcMigrationDecision,
}

impl QueuedMigrationDecision {
    pub(crate) fn new(
        operation_id: String,
        migration_id: String,
        project_id: String,
        decision: IpcMigrationDecision,
    ) -> Result<Self, String> {
        if operation_id.is_empty() || migration_id.is_empty() || project_id.is_empty() {
            return Err("migration decision identity fields must not be empty".to_owned());
        }

        Ok(Self {
            operation_id,
            migration_id,
            project_id,
            decision,
        })
    }

    pub(crate) fn operation_id(&self) -> &str {
        &self.operation_id
    }

    pub(crate) fn migration_id(&self) -> &str {
        &self.migration_id
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.project_id
    }

    pub(crate) const fn decision(&self) -> IpcMigrationDecision {
        self.decision
    }

    pub(crate) fn payload_json(&self) -> Result<String, String> {
        serde_json::to_string(&PersistedMigrationDecision::from(self))
            .map_err(|error| format!("failed to encode migration decision: {error}"))
    }

    pub(crate) fn from_payload_json(
        operation_id: String,
        payload_json: &str,
    ) -> Result<Self, String> {
        let persisted = serde_json::from_str::<PersistedMigrationDecision>(payload_json)
            .map_err(|error| format!("failed to decode migration decision: {error}"))?;

        Self::new(
            operation_id,
            persisted.migration_id,
            persisted.project_id,
            persisted.decision,
        )
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PersistedMigrationDecision {
    migration_id: String,
    project_id: String,
    decision: IpcMigrationDecision,
}

impl From<&QueuedMigrationDecision> for PersistedMigrationDecision {
    fn from(decision: &QueuedMigrationDecision) -> Self {
        Self {
            migration_id: decision.migration_id.clone(),
            project_id: decision.project_id.clone(),
            decision: decision.decision,
        }
    }
}
