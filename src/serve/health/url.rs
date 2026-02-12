use anyhow::Result;

use crate::config::ServiceConfig;

pub(super) fn health_url_for_target(
    target: &ServiceConfig,
    health_path: Option<&str>,
) -> Result<String> {
    let base = super::super::public_url(target)?;
    let configured_path = health_path
        .or(target.health_path.as_deref())
        .unwrap_or("/up");
    let path = if configured_path.starts_with('/') {
        configured_path.to_owned()
    } else {
        format!("/{configured_path}")
    };

    Ok(format!("{}{}", base.trim_end_matches('/'), path))
}
