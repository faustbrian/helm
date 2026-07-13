/// Exact immutable recovery point selected for one reversible restore.
pub(crate) struct RecoveryPointRestoreOptions<'selection> {
    pub(crate) recovery_point_id: &'selection str,
    pub(crate) service_id: &'selection str,
    pub(crate) logical_resource_id: &'selection str,
    pub(crate) resource_kind: &'selection str,
    pub(crate) updated_at_unix_seconds: i64,
}
