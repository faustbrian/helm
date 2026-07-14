/// Exact installation identity and persistent volumes authorized for deletion.
pub(crate) struct InstallationResourceDeletionOptions<'operation> {
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
    pub(crate) authorized_persistent_volumes: &'operation [String],
}
