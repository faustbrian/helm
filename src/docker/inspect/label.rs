//! docker inspect label module.
//!
//! Contains docker inspect label lookup logic used by Helm command workflows.

use crate::docker::is_dry_run;

use super::command::docker_inspect_format;

#[must_use]
pub(super) fn inspect_label(container_name: &str, key: &str) -> Option<String> {
    if is_dry_run() {
        return Some(String::new());
    }

    let template = format!("{{{{index .Config.Labels \"{key}\"}}}}");
    let output = docker_inspect_format(container_name, &template)?;

    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
