//! docker up args builder module.
//!
//! Contains docker up args builder logic used by Helm command workflows.

use crate::config::ServiceConfig;

mod entrypoint;
mod env;
mod labels;

use entrypoint::append_entrypoint_args;
use env::append_run_options;
use labels::append_labels;

/// Builds run args for command execution.
pub(super) fn build_run_args(service: &ServiceConfig, container_name: &str) -> Vec<String> {
    let mut args = vec![
        "run".to_owned(),
        "-d".to_owned(),
        "--name".to_owned(),
        container_name.to_owned(),
        "-p".to_owned(),
        format!(
            "{}:{}:{}",
            service.host,
            service.port,
            service.default_port()
        ),
    ];

    append_run_options(&mut args, service);
    append_labels(&mut args, service, container_name);
    args.push(service.image.clone());
    append_entrypoint_args(&mut args, service);
    args
}
