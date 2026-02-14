//! docker ops prune module.
//!
//! Contains docker container prune operation used by Helm command workflows.

use anyhow::{Context, Result};
use std::process::Command;

use crate::config::ServiceConfig;
use crate::docker::{
    LABEL_MANAGED, VALUE_MANAGED_TRUE, inspect_label, inspect_status, is_dry_run,
    print_docker_command,
};
use crate::output::{self, LogLevel, Persistence};

use super::common::run_docker_status;

pub(super) fn prune(force: bool, filters: &[String]) -> Result<()> {
    let mut args = vec!["container".to_owned(), "prune".to_owned()];
    if force {
        args.push("--force".to_owned());
    }
    for filter in filters {
        args.push("--filter".to_owned());
        args.push(filter.clone());
    }

    if is_dry_run() {
        preview_global_prune_candidates(filters)?;
    }
    run_docker_status(&args, "Failed to execute docker container prune command")
}

pub(super) fn prune_stopped_container(service: &ServiceConfig) -> Result<()> {
    let container_name = service.container_name()?;
    if is_dry_run() {
        print_docker_command(&["rm".to_owned(), container_name]);
        return Ok(());
    }

    let Some(status) = inspect_status(&container_name) else {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("Skipped pruning container {container_name} because it was not found"),
            Persistence::Persistent,
        );
        return Ok(());
    };

    let Some(managed_label) = inspect_label(&container_name, LABEL_MANAGED) else {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("Skipped pruning container {container_name} because labels are unavailable"),
            Persistence::Persistent,
        );
        return Ok(());
    };
    if !is_helm_managed_label(&managed_label) {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("Skipped pruning container {container_name} because it is not Helm-managed"),
            Persistence::Persistent,
        );
        return Ok(());
    }

    if is_active_status(&status) {
        output::event(
            &service.name,
            LogLevel::Info,
            &format!("Skipped pruning container {container_name} because it is {status}"),
            Persistence::Persistent,
        );
        return Ok(());
    }

    let rm_output = Command::new("docker")
        .args(["rm", &container_name])
        .output()
        .context("Failed to execute docker rm command while pruning")?;

    if rm_output.status.success() {
        output::event(
            &service.name,
            LogLevel::Success,
            &format!("Pruned stopped container {container_name}"),
            Persistence::Persistent,
        );
    } else {
        let stderr = String::from_utf8_lossy(&rm_output.stderr).trim().to_owned();
        anyhow::bail!(
            "Failed to prune container {container_name}: {}",
            if stderr.is_empty() {
                "unknown docker error"
            } else {
                &stderr
            }
        );
    }

    Ok(())
}

fn is_active_status(status: &str) -> bool {
    matches!(status, "running" | "paused" | "restarting")
}

fn is_helm_managed_label(value: &str) -> bool {
    value == VALUE_MANAGED_TRUE
}

fn preview_global_prune_candidates(filters: &[String]) -> Result<()> {
    let mut args = vec![
        "ps".to_owned(),
        "-a".to_owned(),
        "--filter".to_owned(),
        "status=exited".to_owned(),
        "--filter".to_owned(),
        "status=created".to_owned(),
        "--filter".to_owned(),
        "status=dead".to_owned(),
        "--format".to_owned(),
        "{{.Names}}".to_owned(),
    ];
    let filter_args: Vec<String> = filters
        .iter()
        .flat_map(|value| ["--filter".to_owned(), value.clone()])
        .collect();
    for item in &filter_args {
        args.push(item.clone());
    }

    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = Command::new("docker")
        .args(&arg_refs)
        .output()
        .context("Failed to preview docker prune candidates")?;
    if !output.status.success() {
        output::event(
            "docker",
            LogLevel::Warn,
            "Dry-run preview could not list global prune candidates",
            Persistence::Persistent,
        );
        return Ok(());
    }

    let names: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    if names.is_empty() {
        output::event(
            "docker",
            LogLevel::Info,
            "Global prune dry-run preview found no stopped containers",
            Persistence::Persistent,
        );
        return Ok(());
    }

    output::event(
        "docker",
        LogLevel::Info,
        &format!(
            "Global prune dry-run preview would remove {} stopped container(s): {}",
            names.len(),
            names.join(", ")
        ),
        Persistence::Persistent,
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{is_active_status, is_helm_managed_label};

    #[test]
    fn active_status_detection_matches_runtime_states() {
        assert!(is_active_status("running"));
        assert!(is_active_status("paused"));
        assert!(is_active_status("restarting"));
        assert!(!is_active_status("exited"));
    }

    #[test]
    fn managed_label_detection_requires_true_value() {
        assert!(is_helm_managed_label("true"));
        assert!(!is_helm_managed_label(""));
        assert!(!is_helm_managed_label("false"));
    }
}
