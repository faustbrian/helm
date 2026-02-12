use anyhow::Result;

use crate::{config, serve};

use super::normalize_path::normalize_path;
use super::probe_http_status::probe_http_status;

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
    Ok(serde_json::json!({
        "name": serve_target.name,
        "app_url": app_url,
        "health_url": health_url,
        "health_status": health_status
    }))
}
