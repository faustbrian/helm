use anyhow::Result;
use rayon::prelude::*;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::super::target_exec::{OutputMode, SwarmRunResult};
use super::super::targets::ResolvedSwarmTarget;
use crate::cli::args::PortStrategyArg;

#[allow(clippy::too_many_arguments)]
pub(super) fn run_targets<F>(
    targets: &[ResolvedSwarmTarget],
    command: &[String],
    output_mode: OutputMode,
    parallel: usize,
    fail_fast: bool,
    port_strategy: PortStrategyArg,
    port_seed: Option<&str>,
    env_output: bool,
    quiet: bool,
    no_color: bool,
    dry_run: bool,
    repro: bool,
    runtime_env: Option<&str>,
    helm_executable: &Path,
    run_target: F,
) -> Result<(Vec<SwarmRunResult>, usize)>
where
    F: Fn(
            &Path,
            &ResolvedSwarmTarget,
            &[String],
            OutputMode,
            Option<Arc<AtomicBool>>,
        ) -> Result<SwarmRunResult>
        + Sync,
{
    if parallel <= 1 {
        let mut results = Vec::new();
        for target in targets {
            let args = super::args::swarm_child_args(
                target,
                command,
                port_strategy,
                port_seed,
                env_output,
                quiet,
                no_color,
                dry_run,
                repro,
                runtime_env,
            );
            let result = run_target(helm_executable, target, &args, output_mode, None)?;
            let failed = !result.success;
            results.push(result);
            if failed && fail_fast {
                break;
            }
        }
        return Ok((results, 0));
    }

    let stop = Arc::new(AtomicBool::new(false));
    let total = targets.len();
    let results = rayon::ThreadPoolBuilder::new()
        .num_threads(parallel)
        .build()?
        .install(|| {
            targets
                .par_iter()
                .filter_map(|target| {
                    if stop.load(Ordering::Relaxed) {
                        return None;
                    }
                    let args = super::args::swarm_child_args(
                        target,
                        command,
                        port_strategy,
                        port_seed,
                        env_output,
                        quiet,
                        no_color,
                        dry_run,
                        repro,
                        runtime_env,
                    );
                    let result = run_target(
                        helm_executable,
                        target,
                        &args,
                        output_mode,
                        Some(stop.clone()),
                    );
                    if fail_fast && result.as_ref().map(|run| !run.success).unwrap_or(true) {
                        stop.store(true, Ordering::Relaxed);
                    }
                    Some(result)
                })
                .collect::<Result<Vec<_>>>()
        })?;

    let cancelled = total.saturating_sub(results.len());
    Ok((results, cancelled))
}
