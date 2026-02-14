//! docker inspect command module.
//!
//! Contains docker inspect command logic used by Helm command workflows.

use std::process::{Command, Output};

pub(super) fn docker_inspect_format(container_name: &str, format: &str) -> Option<Output> {
    Command::new("docker")
        .args(["inspect", &format!("--format={format}"), container_name])
        .output()
        .ok()
}

pub(super) fn docker_inspect(container_name: &str) -> Option<Output> {
    Command::new("docker")
        .args(["inspect", container_name])
        .output()
        .ok()
}
