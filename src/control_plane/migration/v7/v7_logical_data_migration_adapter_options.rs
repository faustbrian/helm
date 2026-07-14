use super::{V7LogicalDataMigrationSource, V7RecoverableMigrationProvider};
use crate::control_plane::state::AcceptedV7InventoryRecord;

/// Accepted source and driver-specific provider for one logical-data checkpoint.
pub(crate) struct V7LogicalDataMigrationAdapterOptions<'operation> {
    pub(crate) accepted: &'operation AcceptedV7InventoryRecord,
    pub(crate) source: &'operation V7LogicalDataMigrationSource,
    pub(crate) provider:
        Box<dyn V7RecoverableMigrationProvider<V7LogicalDataMigrationSource> + 'operation>,
}
