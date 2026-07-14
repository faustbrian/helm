use super::V7NamedVolumeMigrationSource;
use crate::control_plane::engine::{OwnedContainer, OwnedVolume};
use crate::control_plane::state::AcceptedV7InventoryRecord;
use crate::control_plane::workload::DedicatedProjectServicePlan;
use std::path::Path;
use std::time::Duration;

/// Complete immutable context for one recovery-first named-volume transition.
pub(crate) struct V7NamedVolumeMigrationProviderOptions<'operation> {
    pub(crate) accepted: &'operation AcceptedV7InventoryRecord,
    pub(crate) source: &'operation V7NamedVolumeMigrationSource,
    pub(crate) target_container: &'operation OwnedContainer,
    pub(crate) target_volume: &'operation OwnedVolume,
    pub(crate) target_plan: &'operation DedicatedProjectServicePlan,
    pub(crate) backup_root: &'operation Path,
    pub(crate) created_at_unix_seconds: i64,
    pub(crate) verified_at_unix_seconds: i64,
    pub(crate) timeout: Duration,
}
