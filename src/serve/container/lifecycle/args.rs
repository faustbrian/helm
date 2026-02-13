//! `docker run` argument assembly for serve containers.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::config::ServiceConfig;

use super::super::super::mailhog_smtp_port;
use super::super::run_config::{resolve_volume_mapping, resolved_run_command};

/// Builds the full `docker run` argument list for a serve target.
///
/// Injected env values are added before service-defined env values so explicit
/// service env can override inferred defaults.
pub(super) fn build_run_args(
    target: &ServiceConfig,
    runtime_image: &str,
    project_root: &Path,
    injected_env: &HashMap<String, String>,
    inject_server_name: bool,
) -> Result<Vec<String>> {
    let mut run_args = vec![
        "run".to_owned(),
        "-d".to_owned(),
        "--name".to_owned(),
        target.container_name()?,
        "-p".to_owned(),
        format!(
            "{}:{}:{}",
            target.host,
            target.port,
            target.resolved_container_port()
        ),
    ];
    if let Some(smtp_port) = mailhog_smtp_port(target) {
        run_args.push("-p".to_owned());
        run_args.push(format!("{}:{}:1025", target.host, smtp_port));
    }

    if let Some(volumes) = &target.volumes {
        for volume in volumes {
            run_args.push("-v".to_owned());
            run_args.push(resolve_volume_mapping(volume, project_root)?);
        }
    }

    for (key, value) in injected_env {
        run_args.push("-e".to_owned());
        run_args.push(format!("{key}={value}"));
    }

    if let Some(env_vars) = &target.env {
        for (key, value) in env_vars {
            run_args.push("-e".to_owned());
            run_args.push(format!("{key}={value}"));
        }
    }

    if inject_server_name {
        run_args.push("-e".to_owned());
        run_args.push("SERVER_NAME=:80".to_owned());
    }

    run_args.push(runtime_image.to_owned());

    if let Some(command) = resolved_run_command(target) {
        run_args.extend(command);
    }

    Ok(run_args)
}
