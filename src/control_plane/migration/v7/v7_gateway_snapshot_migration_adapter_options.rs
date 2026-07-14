use crate::control_plane::gateway::{GatewayConfiguration, GatewaySnapshot};
use std::path::Path;

/// Live gateway provider and complete before/after snapshots for one cutover.
pub(crate) struct V7GatewaySnapshotMigrationAdapterOptions<'operation> {
    pub(crate) provider: &'operation mut dyn GatewayConfiguration,
    pub(crate) rollback_snapshot: &'operation GatewaySnapshot,
    pub(crate) target_snapshot: &'operation GatewaySnapshot,
    pub(crate) backup_root: &'operation Path,
    pub(crate) created_at_unix_seconds: i64,
    pub(crate) verified_at_unix_seconds: i64,
}
