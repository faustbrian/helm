use serde::{Deserialize, Serialize};

/// Secret-free durable progress for one reversible resource migration.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcMigrationStatus {
    migration_id: String,
    phase: String,
    backup_verified: bool,
    awaiting_confirmation: bool,
    updated_at_unix_seconds: i64,
}

impl IpcMigrationStatus {
    pub(crate) const fn new(
        migration_id: String,
        phase: String,
        backup_verified: bool,
        awaiting_confirmation: bool,
        updated_at_unix_seconds: i64,
    ) -> Self {
        Self {
            migration_id,
            phase,
            backup_verified,
            awaiting_confirmation,
            updated_at_unix_seconds,
        }
    }

    pub(crate) fn migration_id(&self) -> &str {
        &self.migration_id
    }

    pub(crate) fn phase(&self) -> &str {
        &self.phase
    }

    pub(crate) const fn backup_verified(&self) -> bool {
        self.backup_verified
    }

    pub(crate) const fn awaiting_confirmation(&self) -> bool {
        self.awaiting_confirmation
    }

    pub(crate) const fn updated_at_unix_seconds(&self) -> i64 {
        self.updated_at_unix_seconds
    }
}
