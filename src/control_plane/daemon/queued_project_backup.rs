use serde::{Deserialize, Serialize};

/// Secret-free identity of one project recovery point awaiting execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QueuedProjectBackup {
    operation_id: String,
    project_id: String,
    service_id: String,
    logical_resource_id: String,
    kind: String,
    compatibility_fingerprint: String,
}

impl QueuedProjectBackup {
    pub(crate) fn new(
        operation_id: String,
        project_id: String,
        service_id: String,
        logical_resource_id: String,
        kind: String,
        compatibility_fingerprint: String,
    ) -> Result<Self, String> {
        let operation = Self {
            operation_id,
            project_id,
            service_id,
            logical_resource_id,
            kind,
            compatibility_fingerprint,
        };
        operation.validate()?;

        Ok(operation)
    }

    pub(crate) fn operation_id(&self) -> &str {
        &self.operation_id
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

    pub(crate) fn kind(&self) -> &str {
        &self.kind
    }

    pub(crate) fn compatibility_fingerprint(&self) -> &str {
        &self.compatibility_fingerprint
    }

    pub(crate) fn payload_json(&self) -> Result<String, String> {
        serde_json::to_string(&PersistedProjectBackup::from(self))
            .map_err(|error| format!("failed to encode project backup: {error}"))
    }

    pub(crate) fn from_payload_json(
        operation_id: String,
        payload_json: &str,
    ) -> Result<Self, String> {
        let persisted = serde_json::from_str::<PersistedProjectBackup>(payload_json)
            .map_err(|error| format!("failed to decode project backup: {error}"))?;

        persisted.into_queued(operation_id)
    }

    fn validate(&self) -> Result<(), String> {
        if self.operation_id.is_empty()
            || self.project_id.is_empty()
            || self.service_id.is_empty()
            || self.logical_resource_id.is_empty()
            || self.kind.is_empty()
            || self.compatibility_fingerprint.is_empty()
        {
            return Err("project backup identity fields must not be empty".to_owned());
        }
        if !matches!(
            self.kind.as_str(),
            "postgres_database_and_role"
                | "mysql_database"
                | "mariadb_database"
                | "mongodb_database"
                | "sqlserver_database"
                | "redis_acl_prefix"
                | "valkey_acl_prefix"
                | "rabbitmq_vhost_user"
                | "minio_bucket_policy"
        ) {
            return Err(format!(
                "project backup kind '{}' is not implemented",
                self.kind
            ));
        }

        Ok(())
    }
}

/// Stable on-disk representation excluding all credential values.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PersistedProjectBackup {
    project_id: String,
    service_id: String,
    logical_resource_id: String,
    kind: String,
    compatibility_fingerprint: String,
}

impl From<&QueuedProjectBackup> for PersistedProjectBackup {
    fn from(operation: &QueuedProjectBackup) -> Self {
        Self {
            project_id: operation.project_id.clone(),
            service_id: operation.service_id.clone(),
            logical_resource_id: operation.logical_resource_id.clone(),
            kind: operation.kind.clone(),
            compatibility_fingerprint: operation.compatibility_fingerprint.clone(),
        }
    }
}

impl PersistedProjectBackup {
    fn into_queued(self, operation_id: String) -> Result<QueuedProjectBackup, String> {
        QueuedProjectBackup::new(
            operation_id,
            self.project_id,
            self.service_id,
            self.logical_resource_id,
            self.kind,
            self.compatibility_fingerprint,
        )
    }
}
