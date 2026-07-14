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
    convergence_proven: bool,
}

impl<'runtime> EngineBenchmarkSnapshotProvider<'runtime> {
    pub(crate) fn new(
        runtime: &'runtime tokio::runtime::Runtime,
        engine: BollardEngineAdapter,
        installation_id: String,
        schema_version: u32,
        convergence_proven: bool,
    ) -> Self {
        Self {
            runtime,
            engine,
            installation_id,
            schema_version,
            convergence_proven,
        }
    }
}

impl BenchmarkSnapshotProvider for EngineBenchmarkSnapshotProvider<'_> {
    fn snapshot(
        &mut self,
        project_ids: Vec<String>,
        observed_at_unix_seconds: i64,
        require_converged: bool,
    ) -> Result<IpcBenchmarkSnapshot, String> {
        validate_convergence(require_converged, self.convergence_proven)?;
        self.runtime.block_on(collect_benchmark_snapshot(
            &self.engine,
            &self.installation_id,
            self.schema_version,
            project_ids,
            observed_at_unix_seconds,
        ))
    }
}

fn validate_convergence(require_converged: bool, convergence_proven: bool) -> Result<(), String> {
    if require_converged && !convergence_proven {
        return Err(
            "benchmark evidence requires a successful current Engine reconciliation".to_owned(),
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_convergence;

    #[test]
    fn evidence_fails_closed_without_current_engine_convergence() {
        let error = validate_convergence(true, false)
            .expect_err("stale desired state must block benchmark evidence");

        assert_eq!(
            error,
            "benchmark evidence requires a successful current Engine reconciliation"
        );
        assert!(validate_convergence(false, false).is_ok());
        assert!(validate_convergence(true, true).is_ok());
    }
}
