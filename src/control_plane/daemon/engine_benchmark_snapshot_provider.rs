use super::collect_benchmark_snapshot;
use super::ipc::IpcBenchmarkSnapshot;
use super::{BenchmarkSnapshotProvider, EngineBenchmarkSnapshotProviderOptions};
use crate::control_plane::engine::BollardEngineAdapter;

/// Bridges synchronous IPC dispatch to exact owned Engine resource samples.
pub(crate) struct EngineBenchmarkSnapshotProvider<'runtime> {
    runtime: &'runtime tokio::runtime::Runtime,
    engine: BollardEngineAdapter,
    installation_id: String,
    schema_version: u32,
    convergence_proven: bool,
    mutation_in_flight: bool,
}

impl<'runtime> EngineBenchmarkSnapshotProvider<'runtime> {
    pub(crate) fn new(options: EngineBenchmarkSnapshotProviderOptions<'runtime>) -> Self {
        Self {
            runtime: options.runtime,
            engine: options.engine,
            installation_id: options.installation_id,
            schema_version: options.schema_version,
            convergence_proven: options.convergence_proven,
            mutation_in_flight: options.mutation_in_flight,
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
        validate_sampling(
            require_converged,
            self.convergence_proven,
            self.mutation_in_flight,
        )?;
        self.runtime.block_on(collect_benchmark_snapshot(
            &self.engine,
            &self.installation_id,
            self.schema_version,
            project_ids,
            observed_at_unix_seconds,
        ))
    }
}

fn validate_sampling(
    require_converged: bool,
    convergence_proven: bool,
    mutation_in_flight: bool,
) -> Result<(), String> {
    if mutation_in_flight {
        return Err(
            "benchmark sampling is unavailable while an Engine mutation is in flight".to_owned(),
        );
    }
    if require_converged && !convergence_proven {
        return Err(
            "benchmark evidence requires a successful current Engine reconciliation".to_owned(),
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_sampling;

    #[test]
    fn evidence_fails_closed_without_current_engine_convergence() {
        let error = validate_sampling(true, false, false)
            .expect_err("stale desired state must block benchmark evidence");

        assert_eq!(
            error,
            "benchmark evidence requires a successful current Engine reconciliation"
        );
        assert!(validate_sampling(false, false, false).is_ok());
        assert!(validate_sampling(true, true, false).is_ok());
    }

    #[test]
    fn sampling_fails_fast_while_an_engine_mutation_is_active() {
        let error = validate_sampling(false, true, true)
            .expect_err("active restore must prevent synchronous sampling");

        assert_eq!(
            error,
            "benchmark sampling is unavailable while an Engine mutation is in flight"
        );
    }
}
