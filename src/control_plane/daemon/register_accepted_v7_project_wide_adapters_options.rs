use crate::control_plane::migration::{
    V7GatewaySnapshotMigrationAdapterOptions, V7InstallationTrustMigrationAdapterOptions,
};
use crate::control_plane::state::{AcceptedV7InventoryRecord, ManagedEnvironmentRecord};

/// Complete optional capabilities for project-wide accepted-v7 transitions.
pub(crate) struct RegisterAcceptedV7ProjectWideAdaptersOptions<'operation> {
    pub(crate) accepted: &'operation AcceptedV7InventoryRecord,
    pub(crate) gateway: Option<V7GatewaySnapshotMigrationAdapterOptions<'operation>>,
    pub(crate) trust: Option<V7InstallationTrustMigrationAdapterOptions<'operation>>,
    pub(crate) managed_environments: &'operation [ManagedEnvironmentRecord],
    pub(crate) verified_at_unix_seconds: i64,
    pub(crate) maximum_environment_bytes: usize,
}
