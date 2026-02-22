//! cli support run doctor docker module.
//!
//! Contains cli support run doctor docker logic used by Helm command workflows.

use super::report;

/// Checks docker availability and reports actionable failures.
pub(super) fn check_docker_availability() -> bool {
    let cli_failed = run_docker_check(
        "--version",
        "Docker CLI available",
        "Docker unavailable",
        "Docker unavailable",
    );
    let daemon_failed = run_docker_check(
        "info",
        "Docker daemon reachable",
        "Docker daemon not reachable",
        "Docker info failed",
    );

    cli_failed || daemon_failed
}

fn run_docker_check(
    arg: &str,
    success_message: &str,
    failed_output_prefix: &str,
    failed_exec_prefix: &str,
) -> bool {
    let output = crate::docker::run_docker_output(&[arg], "failed to execute docker check");
    match output {
        Ok(output) if output.status.success() => {
            report::success(success_message);
            false
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            report::error(&format!("{failed_output_prefix}: {stderr}"));
            true
        }
        Err(err) => {
            report::error(&format!("{failed_exec_prefix}: {err}"));
            true
        }
    }
}
