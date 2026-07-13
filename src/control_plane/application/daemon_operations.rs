use super::ControlPlane;
use crate::control_plane::state::{
    DaemonEventRecord, DaemonOperationRecord, DaemonOperationTransitionOptions, StateStore,
    StateStoreError,
};

impl<Store> ControlPlane<Store>
where
    Store: StateStore,
{
    pub(crate) fn enqueue_daemon_operation(
        &mut self,
        operation: &DaemonOperationRecord,
        accepted_kind_json: &str,
        event_retention_limit: usize,
    ) -> Result<DaemonEventRecord, StateStoreError> {
        self.state_store.enqueue_daemon_operation(
            operation,
            accepted_kind_json,
            event_retention_limit,
        )
    }

    pub(crate) fn transition_daemon_operation(
        &mut self,
        options: DaemonOperationTransitionOptions<'_>,
    ) -> Result<Option<DaemonEventRecord>, StateStoreError> {
        self.state_store.transition_daemon_operation(options)
    }
}
