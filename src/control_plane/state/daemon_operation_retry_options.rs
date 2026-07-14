/// Exact failed operation identity authorized for one durable retry.
pub(crate) struct DaemonOperationRetryOptions<'operation> {
    pub(crate) operation_id: &'operation str,
    pub(crate) expected_kind: &'operation str,
    pub(crate) expected_payload_json: &'operation str,
    pub(crate) updated_at_unix_seconds: i64,
    pub(crate) accepted_kind_json: &'operation str,
    pub(crate) event_retention_limit: usize,
}
