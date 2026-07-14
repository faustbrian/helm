use super::{V7NamedVolumeMigrationProvider, V7NamedVolumeMigrationSource};
use crate::control_plane::state::AcceptedV7InventoryRecord;

/// Accepted source identity and live provider for one named-volume checkpoint.
pub(crate) struct V7NamedVolumeMigrationAdapterOptions<'operation> {
    pub(crate) accepted: &'operation AcceptedV7InventoryRecord,
    pub(crate) source: &'operation V7NamedVolumeMigrationSource,
    pub(crate) provider: &'operation mut dyn V7NamedVolumeMigrationProvider,
}
