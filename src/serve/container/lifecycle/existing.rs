//! Existing-container reuse strategy for serve lifecycle.

use anyhow::Result;

use crate::output::{self, LogLevel, Persistence};

use super::docker_cmd::{checked_output, force_remove_container};

/// Attempts to reuse an existing container and returns whether work is complete.
///
/// Returns `Ok(true)` when the target container is already running or was
/// successfully started.
pub(super) fn handle_existing_container(container_name: &str) -> Result<bool> {
    let Some(status) = crate::docker::inspect_status(container_name) else {
        return Ok(false);
    };

    if status == "running" {
        emit_already_running(container_name);
        return Ok(true);
    }

    if status == "exited" || status == "created" {
        start_existing_container(container_name)?;
        emit_started_existing(container_name);
        return Ok(true);
    }

    force_remove_container(container_name);
    Ok(false)
}

fn start_existing_container(container_name: &str) -> Result<()> {
    checked_output(
        &["start", container_name],
        "failed to execute docker start",
        "failed to start existing serve container",
    )
}

fn emit_already_running(container_name: &str) {
    output::event(
        container_name,
        LogLevel::Info,
        "Container already running",
        Persistence::Persistent,
    );
}

fn emit_started_existing(container_name: &str) {
    output::event(
        container_name,
        LogLevel::Success,
        "Started existing container",
        Persistence::Persistent,
    );
}
