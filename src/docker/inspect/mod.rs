//! docker inspect module.
//!
//! Contains docker inspect logic used by Helm command workflows.

use std::collections::HashMap;

use super::is_dry_run;
use command::{docker_inspect, docker_inspect_format};
use label::inspect_label as inspect_label_value;
use parse::extract_host_port_binding_from_inspect;

mod command;
mod label;
mod parse;
#[cfg(test)]
mod tests;

#[must_use]
pub fn inspect_status(container_name: &str) -> Option<String> {
    if is_dry_run() {
        return Some("dry-run".to_owned());
    }

    let output = docker_inspect_format(container_name, "{{.State.Status}}")?;

    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[must_use]
pub fn inspect_env(container_name: &str) -> Option<HashMap<String, String>> {
    if is_dry_run() {
        return Some(HashMap::new());
    }

    let output =
        docker_inspect_format(container_name, "{{range .Config.Env}}{{println .}}{{end}}")?;

    if !output.status.success() {
        return None;
    }

    let mut vars = HashMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        vars.insert(key.to_owned(), value.to_owned());
    }
    Some(vars)
}

#[must_use]
pub fn inspect_host_port_binding(
    container_name: &str,
    container_port: u16,
) -> Option<(String, u16)> {
    if is_dry_run() {
        return None;
    }

    let output = docker_inspect(container_name)?;

    if !output.status.success() {
        return None;
    }

    let payload = String::from_utf8_lossy(&output.stdout);
    extract_host_port_binding_from_inspect(payload.as_ref(), container_port)
}

#[must_use]
pub fn inspect_label(container_name: &str, key: &str) -> Option<String> {
    inspect_label_value(container_name, key)
}

#[must_use]
pub fn inspect_json(container_name: &str) -> Option<serde_json::Value> {
    if is_dry_run() {
        return Some(serde_json::json!({}));
    }

    let output = docker_inspect(container_name)?;
    if !output.status.success() {
        return None;
    }

    let payload = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value = serde_json::from_str(payload.as_ref()).ok()?;
    parsed.as_array()?.first().cloned()
}
