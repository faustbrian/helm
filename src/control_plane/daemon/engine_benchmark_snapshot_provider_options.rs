use crate::control_plane::engine::BollardEngineAdapter;

/// Complete state required to serve one synchronous benchmark request safely.
pub(crate) struct EngineBenchmarkSnapshotProviderOptions<'runtime> {
    pub(crate) runtime: &'runtime tokio::runtime::Runtime,
    pub(crate) engine: BollardEngineAdapter,
    pub(crate) installation_id: String,
    pub(crate) schema_version: u32,
    pub(crate) convergence_proven: bool,
    pub(crate) mutation_in_flight: bool,
}
