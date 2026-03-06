//! Result summarization for swarm runs.

use anyhow::Result;

use crate::output::LogLevel;

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
    emit_summary(output_mode, succeeded, failures.len(), skipped);

    if !failures.is_empty() {
        for failure in failures {
            emit_failure(output_mode, failure);
        }
        anyhow::bail!("swarm command failed");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::summarize_results;
    use crate::swarm::target_exec::{OutputMode, SwarmRunResult};
    use std::path::PathBuf;

    fn result(name: &str, success: bool, skipped: bool) -> SwarmRunResult {
        SwarmRunResult {
            name: name.to_owned(),
            root: PathBuf::from(format!("/tmp/{name}")),
            success,
            skipped,
        }
    }

    #[test]
    fn summarize_results_returns_ok_for_successful_targets() {
        let results = vec![result("alpha", true, false), result("beta", true, false)];
        assert!(summarize_results(&results, 0, OutputMode::Logged).is_ok());
    }

    #[test]
    fn summarize_results_ignores_skipped_in_failure_count() {
        let results = vec![
            result("alpha", false, true),
            result("beta", true, false),
            result("gamma", false, false),
        ];
        assert!(summarize_results(&results, 1, OutputMode::Passthrough).is_err());
    }

    #[test]
    fn summarize_results_fails_when_any_target_fails() {
        let results = vec![result("alpha", false, false)];
        assert!(summarize_results(&results, 0, OutputMode::Logged).is_err());
    }
}

fn emit_summary(output_mode: OutputMode, succeeded: usize, failed: usize, skipped: usize) {
    let level = if failed == 0 {
        LogLevel::Success
    } else {
        LogLevel::Error
    };
    let message = match output_mode {
        OutputMode::Logged => {
            format!("Summary: {succeeded} succeeded, {failed} failed, {skipped} skipped")
        }
        OutputMode::Passthrough => {
            format!("Swarm summary: {succeeded} succeeded, {failed} failed, {skipped} skipped")
        }
    };
    super::output::emit_swarm(output_mode, level, &message);
}

fn emit_failure(output_mode: OutputMode, failure: &SwarmRunResult) {
    let message = format!(
        "Failed target '{}' at {}",
        failure.name,
        failure.root.display()
    );
    super::output::emit_swarm(output_mode, LogLevel::Error, &message);
}
