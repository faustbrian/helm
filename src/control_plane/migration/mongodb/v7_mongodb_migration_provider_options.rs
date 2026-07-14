use super::V7MongoDbCredential;
use crate::control_plane::engine::OwnedContainer;
use crate::control_plane::migration::V7LogicalDataMigrationSource;
use crate::control_plane::shared_infrastructure::MongoDbLogicalResourcePlan;
use crate::control_plane::state::{
    AcceptedV7InventoryRecord, CredentialRecord, LogicalResourceRecord,
};
use std::fmt::{Debug, Formatter};
use std::path::Path;
use std::time::Duration;

/// Complete immutable context for one recovery-first v7 MongoDB transition.
pub(crate) struct V7MongoDbMigrationProviderOptions<'operation> {
    pub(crate) accepted: &'operation AcceptedV7InventoryRecord,
    pub(crate) source: &'operation V7LogicalDataMigrationSource,
    pub(crate) source_credential: &'operation V7MongoDbCredential,
    pub(crate) target_container: &'operation OwnedContainer,
    pub(crate) target_logical_resource: &'operation LogicalResourceRecord,
    pub(crate) target_credential: &'operation CredentialRecord,
    pub(crate) target_plan: &'operation MongoDbLogicalResourcePlan,
    pub(crate) administrator: &'operation CredentialRecord,
    pub(crate) installation_id: &'operation str,
    pub(crate) backup_root: &'operation Path,
    pub(crate) created_at_unix_seconds: i64,
    pub(crate) verified_at_unix_seconds: i64,
    pub(crate) timeout: Duration,
}

impl Debug for V7MongoDbMigrationProviderOptions<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("V7MongoDbMigrationProviderOptions")
            .field("accepted", &self.accepted)
            .field("source", &self.source)
            .field("source_credential", &self.source_credential)
            .field("target_container", &self.target_container)
            .field("target_logical_resource", &self.target_logical_resource)
            .field("target_credential", &"[REDACTED]")
            .field("target_plan", &self.target_plan)
            .field("administrator", &"[REDACTED]")
            .field("installation_id", &self.installation_id)
            .field("backup_root", &self.backup_root)
            .field("created_at_unix_seconds", &self.created_at_unix_seconds)
            .field("verified_at_unix_seconds", &self.verified_at_unix_seconds)
            .field("timeout", &self.timeout)
            .finish()
    }
}
