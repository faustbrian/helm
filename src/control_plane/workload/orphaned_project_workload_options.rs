use crate::control_plane::state::ResourceRecord;

/// Durable scope used to stop removed project workloads without deleting them.
pub(crate) struct OrphanedProjectWorkloadOptions<'operation> {
    pub(crate) resources: &'operation [ResourceRecord],
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
}
