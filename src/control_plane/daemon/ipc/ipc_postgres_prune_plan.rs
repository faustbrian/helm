use super::IpcPostgresPrunePlanOptions;
use crate::control_plane::retention::{DataLifecycleStrategy, LogicalPrunePlan};
use serde::{Deserialize, Serialize};

/// Secret-free exact intent returned before destructive logical-resource work.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcPostgresPrunePlan {
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

impl IpcPostgresPrunePlan {
    pub(crate) fn new(options: IpcPostgresPrunePlanOptions) -> Result<Self, String> {
        if !matches!(
            options.strategy,
            DataLifecycleStrategy::PostgreSqlLogical | DataLifecycleStrategy::MySqlLogical
        ) {
            return Err("logical prune strategy has no destructive adapter".to_owned());
        }
        let fields = [
            options.project_id.as_str(),
            options.service_id.as_str(),
            options.logical_resource_id.as_str(),
            options.shared_resource_id.as_str(),
            options.compatibility_fingerprint.as_str(),
            options.credential_id.as_str(),
            options.recovery_point_id.as_str(),
        ];
        if fields.iter().any(|field| field.is_empty()) {
            return Err("logical prune plan identity must not be empty".to_owned());
        }
        if options.confirmation_token.len() != 64
            || !options
                .confirmation_token
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(
                "logical prune confirmation token must be 64 lowercase hexadecimal characters"
                    .to_owned(),
            );
        }

        Ok(Self {
            strategy: options.strategy,
            project_id: options.project_id,
            service_id: options.service_id,
            logical_resource_id: options.logical_resource_id,
            shared_resource_id: options.shared_resource_id,
            compatibility_fingerprint: options.compatibility_fingerprint,
            credential_id: options.credential_id,
            recovery_point_id: options.recovery_point_id,
            confirmation_token: options.confirmation_token,
        })
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
    pub(crate) fn shared_resource_id(&self) -> &str {
        &self.shared_resource_id
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
    pub(crate) fn confirmation_token(&self) -> &str {
        &self.confirmation_token
    }
}

impl From<&LogicalPrunePlan> for IpcPostgresPrunePlan {
    fn from(plan: &LogicalPrunePlan) -> Self {
        Self {
            strategy: plan.strategy(),
            project_id: plan.project_id().to_owned(),
            service_id: plan.service_id().to_owned(),
            logical_resource_id: plan.logical_resource_id().to_owned(),
            shared_resource_id: plan.shared_resource_id().to_owned(),
            compatibility_fingerprint: plan.compatibility_fingerprint().to_owned(),
            credential_id: plan.credential_id().to_owned(),
            recovery_point_id: plan.recovery_point_id().to_owned(),
            confirmation_token: plan.confirmation_token().to_owned(),
        }
    }
}
