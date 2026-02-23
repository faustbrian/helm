//! Run-command and volume mapping derivation for serve containers.

use anyhow::Result;
use std::path::Path;

use crate::config::{Driver, ServiceConfig};

/// Resolves the container command passed after the image name.
///
/// Uses explicit `service.command` when present; otherwise derives a FrankenPHP
/// Octane command when Octane mode is enabled.
pub(crate) fn resolved_run_command(target: &ServiceConfig) -> Option<Vec<String>> {
    if let Some(command) = &target.command {
        return Some(command.clone());
    }

    if target.octane && target.driver == Driver::Frankenphp {
        return Some(vec![
            "php".to_owned(),
            "artisan".to_owned(),
            "octane:frankenphp".to_owned(),
            "--ansi".to_owned(),
            "--watch".to_owned(),
            "--workers=1".to_owned(),
            "--max-requests=1".to_owned(),
            "--host=0.0.0.0".to_owned(),
            format!("--port={}", target.resolved_container_port()),
        ]);
    }

    None
}

/// Resolves a `host:container[:mode]` mapping into its final Docker form.
///
/// Relative host paths are expanded against `project_root`.
pub(super) fn resolve_volume_mapping(volume: &str, project_root: &Path) -> Result<String> {
    let mut parts = volume.splitn(3, ':');
    let host = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("invalid volume mapping '{volume}'"))?;
    let container = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("invalid volume mapping '{volume}'"))?;
    let mode = parts.next();

    let host_path = Path::new(host);
    let host_segment = if host_path.is_absolute() || host.starts_with('~') {
        host.to_owned()
    } else {
        project_root.join(host).display().to_string()
    };

    let mut mapping = format!("{host_segment}:{container}");
    if let Some(mode_value) = mode {
        mapping.push(':');
        mapping.push_str(mode_value);
    }

    Ok(mapping)
}
