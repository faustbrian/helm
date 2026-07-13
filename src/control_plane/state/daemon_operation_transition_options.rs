use super::DaemonOperationStatus;

/// Complete guarded transition for one exact durable daemon operation.
#[derive(Clone, Copy)]
pub(crate) struct DaemonOperationTransitionOptions<'operation> {
    pub(crate) operation_id: &'operation str,
    pub(crate) expected: DaemonOperationStatus,
    pub(crate) next: DaemonOperationStatus,
    pub(crate) updated_at_unix_seconds: i64,
    pub(crate) event_kind_json: Option<&'operation str>,
    pub(crate) event_retention_limit: usize,
}
