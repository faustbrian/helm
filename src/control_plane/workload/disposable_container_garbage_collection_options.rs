use crate::control_plane::state::ResourceRecord;

/// Complete policy and ownership input for one disposable-container GC pass.
pub(crate) struct DisposableContainerGarbageCollectionOptions<'resources> {
    pub(crate) resources: &'resources [ResourceRecord],
    pub(crate) installation_id: &'resources str,
    pub(crate) schema_version: u32,
    pub(crate) now_unix_seconds: i64,
    pub(crate) orphan_retention_seconds: i64,
}
