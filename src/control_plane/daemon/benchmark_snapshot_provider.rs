use super::ipc::IpcBenchmarkSnapshot;

/// Synchronous IPC seam for one complete read-only Engine metrics sample.
pub(crate) trait BenchmarkSnapshotProvider {
    fn snapshot(
        &mut self,
        project_count: usize,
        observed_at_unix_seconds: i64,
    ) -> Result<IpcBenchmarkSnapshot, String>;
}
