//! Result summarization for swarm runs.

use anyhow::Result;

use crate::output::{self, LogLevel, Persistence};

use super::super::target_exec::{OutputMode, SwarmRunResult};

/// Emits run summary output and converts failed targets into a terminal error.
///
/// Any non-skipped failure causes this function to return an error after
/// printing/logging per-target failure entries.
pub(super) fn summarize_results(
    results: &[SwarmRunResult],
    cancelled: usize,
    output_mode: OutputMode,
) -> Result<()> {
    let failures: Vec<&SwarmRunResult> = results
        .iter()
        .filter(|result| !result.success && !result.skipped)
        .collect();
    let skipped = results.iter().filter(|result| result.skipped).count() + cancelled;
    let succeeded = results
        .iter()
        .filter(|result| result.success && !result.skipped)
        .count();

    match output_mode {
        OutputMode::Logged => output::event(
            "swarm",
            if failures.is_empty() {
                LogLevel::Success
            } else {
                LogLevel::Error
            },
            &format!(
                "Summary: {succeeded} succeeded, {} failed, {skipped} skipped",
                failures.len()
            ),
            Persistence::Persistent,
        ),
        OutputMode::Passthrough => {
            println!(
                "Swarm summary: {succeeded} succeeded, {} failed, {skipped} skipped",
                failures.len()
            );
        }
    }
    if !failures.is_empty() {
        for failure in failures {
            match output_mode {
                OutputMode::Logged => output::event(
                    "swarm",
                    LogLevel::Error,
                    &format!(
                        "Failed target '{}' at {}",
                        failure.name,
                        failure.root.display()
                    ),
                    Persistence::Persistent,
                ),
                OutputMode::Passthrough => {
                    eprintln!(
                        "Failed target '{}' at {}",
                        failure.name,
                        failure.root.display()
                    );
                }
            }
        }
        anyhow::bail!("swarm command failed");
    }

    Ok(())
}
