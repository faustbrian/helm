use crate::control_plane::retention::{DataLifecycleStrategy, PostgresLogicalPrunePlan};
use serde::{Deserialize, Serialize};

/// Secret-free immutable intent for one explicitly confirmed logical prune.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QueuedPostgresPrune {
    operation_id: String,
    strategy: DataLifecycleStrategy,
    project_id: String,
    service_id: String,
    logical_resource_id: String,
    shared_resource_id: String,
    compatibility_fingerprint: String,
    credential_id: String,
    recovery_point_id: String,
    confirmation_token: String,
}

impl QueuedPostgresPrune {
    pub(crate) fn new(
        operation_id: String,
        plan: &PostgresLogicalPrunePlan,
        confirmation_token: String,
    ) -> Result<Self, String> {
        if confirmation_token != plan.confirmation_token() {
            return Err("logical prune confirmation token is stale or incorrect".to_owned());
        }
        let operation = Self {
            operation_id,
            strategy: plan.strategy(),
            project_id: plan.project_id().to_owned(),
            service_id: plan.service_id().to_owned(),
            logical_resource_id: plan.logical_resource_id().to_owned(),
            shared_resource_id: plan.shared_resource_id().to_owned(),
            compatibility_fingerprint: plan.compatibility_fingerprint().to_owned(),
            credential_id: plan.credential_id().to_owned(),
            recovery_point_id: plan.recovery_point_id().to_owned(),
            confirmation_token,
        };
        operation.validate()?;

        Ok(operation)
    }

    pub(crate) fn operation_id(&self) -> &str {
        &self.operation_id
    }
    pub(crate) const fn strategy(&self) -> DataLifecycleStrategy {
        self.strategy
    }
    pub(crate) fn project_id(&self) -> &str {
        &self.project_id
    }
    pub(crate) fn service_id(&self) -> &str {
        &self.service_id
    }
    pub(crate) fn logical_resource_id(&self) -> &str {
        &self.logical_resource_id
    }
    pub(crate) fn compatibility_fingerprint(&self) -> &str {
        &self.compatibility_fingerprint
    }
    pub(crate) fn credential_id(&self) -> &str {
        &self.credential_id
    }
    pub(crate) fn recovery_point_id(&self) -> &str {
        &self.recovery_point_id
    }

    pub(crate) fn matches_plan(&self, plan: &PostgresLogicalPrunePlan) -> bool {
        self.strategy == plan.strategy()
            && self.project_id == plan.project_id()
            && self.service_id == plan.service_id()
            && self.logical_resource_id == plan.logical_resource_id()
            && self.shared_resource_id == plan.shared_resource_id()
            && self.compatibility_fingerprint == plan.compatibility_fingerprint()
            && self.credential_id == plan.credential_id()
            && self.recovery_point_id == plan.recovery_point_id()
            && self.confirmation_token == plan.confirmation_token()
    }

    pub(crate) fn payload_json(&self) -> Result<String, String> {
        serde_json::to_string(&PersistedPostgresPrune::from(self))
            .map_err(|error| format!("failed to encode logical prune: {error}"))
    }

    pub(crate) fn from_payload_json(
        operation_id: String,
        payload_json: &str,
    ) -> Result<Self, String> {
        let persisted = serde_json::from_str::<PersistedPostgresPrune>(payload_json)
            .map_err(|error| format!("failed to decode logical prune: {error}"))?;
        let operation = Self {
            operation_id,
            strategy: persisted.strategy,
            project_id: persisted.project_id,
            service_id: persisted.service_id,
            logical_resource_id: persisted.logical_resource_id,
            shared_resource_id: persisted.shared_resource_id,
            compatibility_fingerprint: persisted.compatibility_fingerprint,
            credential_id: persisted.credential_id,
            recovery_point_id: persisted.recovery_point_id,
            confirmation_token: persisted.confirmation_token,
        };
        operation.validate()?;

        Ok(operation)
    }

    fn validate(&self) -> Result<(), String> {
        let fields = [
            self.operation_id.as_str(),
            self.project_id.as_str(),
            self.service_id.as_str(),
            self.logical_resource_id.as_str(),
            self.shared_resource_id.as_str(),
            self.compatibility_fingerprint.as_str(),
            self.credential_id.as_str(),
            self.recovery_point_id.as_str(),
        ];
        if fields.iter().any(|field| field.is_empty())
            || self.confirmation_token.len() != 64
            || !self
                .confirmation_token
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("logical prune identity is incomplete or malformed".to_owned());
        }

        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PersistedPostgresPrune {
    #[serde(default = "default_postgres_strategy")]
    strategy: DataLifecycleStrategy,
    project_id: String,
    service_id: String,
    logical_resource_id: String,
    shared_resource_id: String,
    compatibility_fingerprint: String,
    credential_id: String,
    recovery_point_id: String,
    confirmation_token: String,
}

impl From<&QueuedPostgresPrune> for PersistedPostgresPrune {
    fn from(operation: &QueuedPostgresPrune) -> Self {
        Self {
            strategy: operation.strategy,
            project_id: operation.project_id.clone(),
            service_id: operation.service_id.clone(),
            logical_resource_id: operation.logical_resource_id.clone(),
            shared_resource_id: operation.shared_resource_id.clone(),
            compatibility_fingerprint: operation.compatibility_fingerprint.clone(),
            credential_id: operation.credential_id.clone(),
            recovery_point_id: operation.recovery_point_id.clone(),
            confirmation_token: operation.confirmation_token.clone(),
        }
    }
}

const fn default_postgres_strategy() -> DataLifecycleStrategy {
    DataLifecycleStrategy::PostgreSqlLogical
}
