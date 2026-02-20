//! Shared docker command helpers for artisan runtime reset.

use anyhow::Result;
use std::process::Output;

/// Best-effort forced container+volume removal.
pub(super) fn try_remove_container_with_volumes(container_name: &str) {
    drop(crate::docker::run_docker_output(
        &["rm", "-f", "-v", container_name],
        "failed to remove runtime test container",
    ));
}

/// Runs `docker volume rm` for a named volume.
pub(super) fn remove_named_volume_output(volume_name: &str) -> Result<Output> {
    crate::docker::run_docker_output(
        &["volume", "rm", volume_name],
        "failed to remove docker volume",
    )
}
