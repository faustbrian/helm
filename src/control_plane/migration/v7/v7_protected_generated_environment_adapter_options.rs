use crate::control_plane::state::{AcceptedV7InventoryRecord, ManagedEnvironmentRecord};

/// Accepted recovery evidence and prepared v8 target for one environment.
pub(crate) struct V7ProtectedGeneratedEnvironmentAdapterOptions<'operation> {
    pub(crate) accepted: &'operation AcceptedV7InventoryRecord,
    pub(crate) target_environment: &'operation ManagedEnvironmentRecord,
    pub(crate) verified_at_unix_seconds: i64,
    pub(crate) maximum_environment_bytes: usize,
}
