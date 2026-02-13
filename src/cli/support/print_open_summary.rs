//! cli support print open summary module.
//!
//! Contains cli support print open summary logic used by Helm command workflows.

use anyhow::Result;

use crate::output::{self, LogLevel, Persistence};
use crate::{config, serve};

use super::normalize_path::normalize_path;
use super::open_in_browser::open_in_browser;
use super::probe_http_status::probe_http_status;

pub(crate) fn print_open_summary(
    serve_target: &config::ServiceConfig,
    health_path: Option<&str>,
    no_browser: bool,
) -> Result<()> {
    let app_url = serve::public_url(serve_target)?;
    output::event(
        &serve_target.name,
        LogLevel::Info,
        &format!("App URL: {app_url}"),
        Persistence::Persistent,
    );

    if !no_browser {
        open_in_browser(&app_url);
    }

    let health_url = format!(
        "{}{}",
        app_url.trim_end_matches('/'),
        normalize_path(
            health_path.unwrap_or_else(|| serve_target.health_path.as_deref().unwrap_or("/up"))
        )
    );
    match probe_http_status(&health_url) {
        Some(code) if (200..=299).contains(&code) => {
            output::event(
                &serve_target.name,
                LogLevel::Success,
                &format!("Health: {health_url} ({code})"),
                Persistence::Persistent,
            );
        }
        Some(code) => {
            output::event(
                &serve_target.name,
                LogLevel::Warn,
                &format!("Health: {health_url} ({code})"),
                Persistence::Persistent,
            );
        }
        None => {
            output::event(
                &serve_target.name,
                LogLevel::Warn,
                &format!("Health: {health_url} (unreachable)"),
                Persistence::Persistent,
            );
        }
    }

    Ok(())
}
