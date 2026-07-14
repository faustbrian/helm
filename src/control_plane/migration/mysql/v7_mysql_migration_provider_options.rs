use super::V7MySqlCredential;
use crate::control_plane::engine::OwnedContainer;
use crate::control_plane::migration::V7LogicalDataMigrationSource;
use crate::control_plane::shared_infrastructure::{MySqlFlavor, MySqlLogicalResourcePlan};
use crate::control_plane::state::{
    AcceptedV7InventoryRecord, CredentialRecord, LogicalResourceRecord,
};
use std::fmt::{Debug, Formatter};
use std::path::Path;
use std::time::Duration;

/// Complete immutable context for a recovery-first v7 MySQL transition.
pub(crate) struct V7MySqlMigrationProviderOptions<'operation> {
    pub(crate) accepted: &'operation AcceptedV7InventoryRecord,
    pub(crate) source: &'operation V7LogicalDataMigrationSource,
    pub(crate) flavor: MySqlFlavor,
    pub(crate) source_credential: &'operation V7MySqlCredential,
    pub(crate) target_container: &'operation OwnedContainer,
    pub(crate) target_logical_resource: &'operation LogicalResourceRecord,
    pub(crate) target_credential: &'operation CredentialRecord,
    pub(crate) target_plan: &'operation MySqlLogicalResourcePlan,
    pub(crate) administrator: &'operation CredentialRecord,
    pub(crate) installation_id: &'operation str,
    pub(crate) backup_root: &'operation Path,
    pub(crate) created_at_unix_seconds: i64,
    pub(crate) verified_at_unix_seconds: i64,
    pub(crate) timeout: Duration,
}

impl Debug for V7MySqlMigrationProviderOptions<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("V7MySqlMigrationProviderOptions")
            .field("accepted", &self.accepted)
            .field("source", &self.source)
            .field("flavor", &self.flavor)
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
