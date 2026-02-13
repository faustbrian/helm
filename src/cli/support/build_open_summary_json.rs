//! cli support build open summary json module.
//!
//! Contains cli support build open summary json logic used by Helm command workflows.

use anyhow::Result;

use crate::{config, serve};

use super::normalize_path::normalize_path;
use super::probe_http_status::probe_http_status;

/// Builds open summary json for command execution.
pub(crate) fn build_open_summary_json(
    serve_target: &config::ServiceConfig,
    health_path: Option<&str>,
) -> Result<serde_json::Value> {
    let app_url = serve::public_url(serve_target)?;
    let health_url = format!(
        "{}{}",
        app_url.trim_end_matches('/'),
        normalize_path(
            health_path.unwrap_or_else(|| serve_target.health_path.as_deref().unwrap_or("/up"))
        )
    );
    let health_status = probe_http_status(&health_url);
    let health_ok = health_status
        .map(|status| health_status_accepted(serve_target, status))
        .unwrap_or(false);
    Ok(serde_json::json!({
        "name": serve_target.name,
        "app_url": app_url,
        "health_url": health_url,
        "health_status": health_status,
        "health_ok": health_ok
    }))
}

fn health_status_accepted(serve_target: &config::ServiceConfig, status: u16) -> bool {
    serve_target
        .health_statuses
        .as_ref()
        .is_some_and(|values| values.contains(&status))
        || (200..300).contains(&status)
}
