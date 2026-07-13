use crate::control_plane::engine::ContainerCreateOptions;

/// Ownership boundary inputs for one project workload reconciliation pass.
pub(crate) struct WorkloadReconcileOptions<'operation> {
    pub(crate) request: &'operation ContainerCreateOptions,
    pub(crate) installation_id: &'operation str,
    pub(crate) schema_version: u32,
}
