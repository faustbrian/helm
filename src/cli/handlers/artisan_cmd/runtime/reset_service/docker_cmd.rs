//! Shared docker command helpers for artisan runtime reset.

use anyhow::Result;
use std::process::Output;

/// Best-effort forced container+volume removal.
pub(super) fn try_remove_container_with_volumes(container_name: &str) {
    let result = crate::docker::with_scheduled_docker_op(
        crate::docker::DockerOpClass::Heavy,
        "docker-rm-runtime-test-container",
        || {
            crate::docker::run_docker_output(
                &["rm", "-f", "-v", container_name],
                "failed to remove runtime test container",
            )
            .map(|_| ())
        },
    );
    drop(result);
}

/// Runs `docker volume rm` for a named volume.
pub(super) fn remove_named_volume_output(volume_name: &str) -> Result<Output> {
    crate::docker::with_scheduled_docker_op(
        crate::docker::DockerOpClass::Heavy,
        "docker-volume-rm-runtime-test",
        || {
            crate::docker::run_docker_output(
                &["volume", "rm", volume_name],
                "failed to remove docker volume",
            )
        },
    )
}
