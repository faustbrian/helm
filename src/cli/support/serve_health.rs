//! Shared serve-health URL and status helpers.

use crate::config::ServiceConfig;

use super::normalize_path::normalize_path;

/// Builds full health URL from app URL and optional override path.
pub(crate) fn build_health_url(
    target: &ServiceConfig,
    app_url: &str,
    health_path: Option<&str>,
) -> String {
    format!(
        "{}{}",
        app_url.trim_end_matches('/'),
        normalize_path(
            health_path.unwrap_or_else(|| target.health_path.as_deref().unwrap_or("/up"))
        )
    )
}

/// Returns true when status is accepted by service policy.
pub(crate) fn health_status_accepted(target: &ServiceConfig, status: u16) -> bool {
    target
        .health_statuses
        .as_ref()
        .is_some_and(|values| values.contains(&status))
        || (200..300).contains(&status)
}
