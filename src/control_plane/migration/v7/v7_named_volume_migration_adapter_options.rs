use super::{V7NamedVolumeMigrationSource, V7RecoverableMigrationProvider};
use crate::control_plane::state::AcceptedV7InventoryRecord;

/// Accepted source identity and live provider for one named-volume checkpoint.
pub(crate) struct V7NamedVolumeMigrationAdapterOptions<'operation> {
    pub(crate) accepted: &'operation AcceptedV7InventoryRecord,
    pub(crate) source: &'operation V7NamedVolumeMigrationSource,
    pub(crate) provider:
        Box<dyn V7RecoverableMigrationProvider<V7NamedVolumeMigrationSource> + 'operation>,
}
