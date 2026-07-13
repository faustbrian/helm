use super::ControlPlane;
use crate::control_plane::state::{DaemonEventRecord, StateStore, StateStoreError};

impl<Store> ControlPlane<Store>
where
    Store: StateStore,
{
    /// Atomically appends one bounded daemon event to authoritative state.
    pub(crate) fn append_daemon_event(
        &mut self,
        operation_id: &str,
        kind_json: &str,
        retention_limit: usize,
    ) -> Result<DaemonEventRecord, StateStoreError> {
        self.state_store
            .append_daemon_event(operation_id, kind_json, retention_limit)
    }
}
