use super::BenchmarkSnapshotProvider;
use super::collect_benchmark_snapshot;
use super::ipc::IpcBenchmarkSnapshot;
use crate::control_plane::engine::BollardEngineAdapter;

/// Bridges synchronous IPC dispatch to exact owned Engine resource samples.
pub(crate) struct EngineBenchmarkSnapshotProvider<'runtime> {
    runtime: &'runtime tokio::runtime::Runtime,
    engine: BollardEngineAdapter,
    installation_id: String,
    schema_version: u32,
}

impl<'runtime> EngineBenchmarkSnapshotProvider<'runtime> {
    pub(crate) fn new(
        runtime: &'runtime tokio::runtime::Runtime,
        engine: BollardEngineAdapter,
        installation_id: String,
        schema_version: u32,
    ) -> Self {
        Self {
            runtime,
            engine,
            installation_id,
            schema_version,
        }
    }
}

impl BenchmarkSnapshotProvider for EngineBenchmarkSnapshotProvider<'_> {
    fn snapshot(
        &mut self,
        project_count: usize,
        observed_at_unix_seconds: i64,
    ) -> Result<IpcBenchmarkSnapshot, String> {
        self.runtime.block_on(collect_benchmark_snapshot(
            &self.engine,
            &self.installation_id,
            self.schema_version,
            project_count,
            observed_at_unix_seconds,
        ))
    }
}
