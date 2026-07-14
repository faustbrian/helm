use super::ipc::IpcBenchmarkSnapshot;

/// Synchronous IPC seam for one complete read-only Engine metrics sample.
pub(crate) trait BenchmarkSnapshotProvider {
    fn snapshot(
        &mut self,
        project_ids: Vec<String>,
        observed_at_unix_seconds: i64,
        require_converged: bool,
    ) -> Result<IpcBenchmarkSnapshot, String>;
}
