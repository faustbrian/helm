/// Complete policy and ownership input for one derived-image GC pass.
pub(crate) struct BuildImageGarbageCollectionOptions<'resources> {
    pub(crate) active_image_ids: &'resources [String],
    pub(crate) installation_id: &'resources str,
    pub(crate) schema_version: u32,
    pub(crate) now_unix_seconds: i64,
    pub(crate) retention_seconds: i64,
}
