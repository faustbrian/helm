use serde::{Deserialize, Serialize};

/// Secret-free identity of one exact recovery point awaiting restoration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct QueuedProjectRestore {
    operation_id: String,
    recovery_point_id: String,
    project_id: String,
    service_id: String,
    logical_resource_id: String,
    kind: String,
    compatibility_fingerprint: String,
}

impl QueuedProjectRestore {
    pub(crate) fn new(options: QueuedProjectRestoreOptions) -> Result<Self, String> {
        let operation = Self {
            operation_id: options.operation_id,
            recovery_point_id: options.recovery_point_id,
            project_id: options.project_id,
            service_id: options.service_id,
            logical_resource_id: options.logical_resource_id,
            kind: options.kind,
            compatibility_fingerprint: options.compatibility_fingerprint,
        };
        operation.validate()?;

        Ok(operation)
    }

    pub(crate) fn operation_id(&self) -> &str {
        &self.operation_id
    }

    pub(crate) fn recovery_point_id(&self) -> &str {
        &self.recovery_point_id
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
        serde_json::to_string(&PersistedProjectRestore::from(self))
            .map_err(|error| format!("failed to encode project restore: {error}"))
    }

    pub(crate) fn from_payload_json(
        operation_id: String,
        payload_json: &str,
    ) -> Result<Self, String> {
        let persisted = serde_json::from_str::<PersistedProjectRestore>(payload_json)
            .map_err(|error| format!("failed to decode project restore: {error}"))?;

        persisted.into_queued(operation_id)
    }

    fn validate(&self) -> Result<(), String> {
        if self.operation_id.is_empty()
            || self.recovery_point_id.is_empty()
            || self.project_id.is_empty()
            || self.service_id.is_empty()
            || self.logical_resource_id.is_empty()
            || self.kind.is_empty()
            || self.compatibility_fingerprint.is_empty()
        {
            return Err("project restore identity fields must not be empty".to_owned());
        }
        if !matches!(
            self.kind.as_str(),
            "postgres_database_and_role" | "mysql_database" | "mariadb_database"
        ) {
            return Err(format!(
                "project restore kind '{}' is not implemented",
                self.kind
            ));
        }

        Ok(())
    }
}

/// Complete validated identity for one queued restore request.
pub(crate) struct QueuedProjectRestoreOptions {
    pub(crate) operation_id: String,
    pub(crate) recovery_point_id: String,
    pub(crate) project_id: String,
    pub(crate) service_id: String,
    pub(crate) logical_resource_id: String,
    pub(crate) kind: String,
    pub(crate) compatibility_fingerprint: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PersistedProjectRestore {
    recovery_point_id: String,
    project_id: String,
    service_id: String,
    logical_resource_id: String,
    kind: String,
    compatibility_fingerprint: String,
}

impl From<&QueuedProjectRestore> for PersistedProjectRestore {
    fn from(operation: &QueuedProjectRestore) -> Self {
        Self {
            recovery_point_id: operation.recovery_point_id.clone(),
            project_id: operation.project_id.clone(),
            service_id: operation.service_id.clone(),
            logical_resource_id: operation.logical_resource_id.clone(),
            kind: operation.kind.clone(),
            compatibility_fingerprint: operation.compatibility_fingerprint.clone(),
        }
    }
}

impl PersistedProjectRestore {
    fn into_queued(self, operation_id: String) -> Result<QueuedProjectRestore, String> {
        QueuedProjectRestore::new(QueuedProjectRestoreOptions {
            operation_id,
            recovery_point_id: self.recovery_point_id,
            project_id: self.project_id,
            service_id: self.service_id,
            logical_resource_id: self.logical_resource_id,
            kind: self.kind,
            compatibility_fingerprint: self.compatibility_fingerprint,
        })
    }
}
