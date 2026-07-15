use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
    dump: Option<QueuedDatabaseDump>,
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
            dump: None,
        };
        operation.validate()?;

        Ok(operation)
    }

    pub(crate) fn from_database_dump(
        options: QueuedDatabaseDumpRestoreOptions,
    ) -> Result<Self, String> {
        let operation = Self {
            operation_id: options.restore.operation_id,
            recovery_point_id: format!("dump:{}", options.restore.service_id),
            project_id: options.restore.project_id,
            service_id: options.restore.service_id,
            logical_resource_id: options.restore.logical_resource_id,
            kind: options.restore.kind,
            compatibility_fingerprint: options.restore.compatibility_fingerprint,
            dump: Some(QueuedDatabaseDump {
                file: options.file,
                archive_entry: options.archive_entry,
                reset: options.reset,
            }),
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

    pub(crate) fn dump_file(&self) -> Option<&Path> {
        self.dump.as_ref().map(|dump| dump.file.as_path())
    }

    pub(crate) fn dump_archive_entry(&self) -> Option<&str> {
        self.dump
            .as_ref()
            .and_then(|dump| dump.archive_entry.as_deref())
    }

    pub(crate) fn resets_database(&self) -> bool {
        self.dump.as_ref().is_some_and(|dump| dump.reset)
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
            "postgres_database_and_role"
                | "mysql_database"
                | "mariadb_database"
                | "mongodb_database"
                | "sqlserver_database"
                | "redis_acl_prefix"
                | "valkey_acl_prefix"
                | "minio_bucket_policy"
                | "rabbitmq_vhost_user"
                | "volume"
        ) {
            return Err(format!(
                "project restore kind '{}' is not implemented",
                self.kind
            ));
        }
        if let Some(dump) = &self.dump {
            if !matches!(self.kind.as_str(), "mysql_database" | "mariadb_database") {
                return Err("database dump restore currently requires MySQL or MariaDB".to_owned());
            }
            if !dump.file.is_absolute() || dump.file.as_os_str().is_empty() {
                return Err("database dump path must be canonical and absolute".to_owned());
            }
            if dump.archive_entry.as_ref().is_some_and(String::is_empty) {
                return Err("database dump archive entry must not be empty".to_owned());
            }
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

pub(crate) struct QueuedDatabaseDumpRestoreOptions {
    pub(crate) restore: QueuedProjectRestoreOptions,
    pub(crate) file: PathBuf,
    pub(crate) archive_entry: Option<String>,
    pub(crate) reset: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct QueuedDatabaseDump {
    file: PathBuf,
    archive_entry: Option<String>,
    reset: bool,
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
    #[serde(default)]
    dump: Option<QueuedDatabaseDump>,
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
            dump: operation.dump.clone(),
        }
    }
}

impl PersistedProjectRestore {
    fn into_queued(self, operation_id: String) -> Result<QueuedProjectRestore, String> {
        let restore = QueuedProjectRestoreOptions {
            operation_id,
            recovery_point_id: self.recovery_point_id,
            project_id: self.project_id,
            service_id: self.service_id,
            logical_resource_id: self.logical_resource_id,
            kind: self.kind,
            compatibility_fingerprint: self.compatibility_fingerprint,
        };
        match self.dump {
            Some(dump) => {
                QueuedProjectRestore::from_database_dump(QueuedDatabaseDumpRestoreOptions {
                    restore,
                    file: dump.file,
                    archive_entry: dump.archive_entry,
                    reset: dump.reset,
                })
            }
            None => QueuedProjectRestore::new(restore),
        }
    }
}
