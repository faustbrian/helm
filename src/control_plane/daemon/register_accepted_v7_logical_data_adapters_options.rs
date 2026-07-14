use super::AcceptedV7LogicalDataInput;
use crate::control_plane::engine::OwnedContainer;
use crate::control_plane::shared_infrastructure::PreparedSharedInstance;
use crate::control_plane::state::{AcceptedV7InventoryRecord, LogicalResourceRecord};
use std::path::Path;
use std::time::Duration;

/// Complete execution-scoped inputs for accepted-v7 logical adapter binding.
pub(crate) struct RegisterAcceptedV7LogicalDataAdaptersOptions<'operation, E> {
    pub(crate) accepted: &'operation AcceptedV7InventoryRecord,
    pub(crate) inputs: &'operation [AcceptedV7LogicalDataInput],
    pub(crate) prepared: &'operation [PreparedSharedInstance],
    pub(crate) target_containers: &'operation [OwnedContainer],
    pub(crate) logical_resources: &'operation [LogicalResourceRecord],
    pub(crate) engine: &'operation E,
    pub(crate) installation_id: &'operation str,
    pub(crate) backup_root: &'operation Path,
    pub(crate) created_at_unix_seconds: i64,
    pub(crate) verified_at_unix_seconds: i64,
    pub(crate) timeout: Duration,
}
