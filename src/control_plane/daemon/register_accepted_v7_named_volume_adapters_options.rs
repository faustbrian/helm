use super::EngineReconciliationPlan;
use crate::control_plane::engine::{OwnedContainer, OwnedVolume};
use crate::control_plane::migration::V7NamedVolumeMigrationSource;
use crate::control_plane::state::AcceptedV7InventoryRecord;
use std::path::Path;
use std::time::Duration;

/// Complete execution-scoped inputs for accepted-v7 volume adapter binding.
pub(crate) struct RegisterAcceptedV7NamedVolumeAdaptersOptions<'operation, E> {
    pub(crate) accepted: &'operation AcceptedV7InventoryRecord,
    pub(crate) sources: &'operation [V7NamedVolumeMigrationSource],
    pub(crate) reconciliation: &'operation EngineReconciliationPlan,
    pub(crate) target_containers: &'operation [OwnedContainer],
    pub(crate) target_volumes: &'operation [OwnedVolume],
    pub(crate) engine: &'operation E,
    pub(crate) backup_root: &'operation Path,
    pub(crate) created_at_unix_seconds: i64,
    pub(crate) verified_at_unix_seconds: i64,
    pub(crate) timeout: Duration,
}
