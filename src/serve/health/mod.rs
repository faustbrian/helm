use anyhow::Result;
use std::collections::HashSet;
use std::thread;
use std::time::Duration;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

mod body;
mod url;

use body::body_health_is_ok;
use url::health_url_for_target;

pub(crate) fn wait_until_http_healthy(
    target: &ServiceConfig,
    timeout_secs: u64,
    interval_secs: u64,
    health_path: Option<&str>,
) -> Result<()> {
    let health_url = health_url_for_target(target, health_path)?;
    let accepted_statuses: HashSet<u16> = target
        .health_statuses
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect();
    output::event(
        &target.name,
        LogLevel::Info,
        &format!("Waiting for health check at {health_url}"),
        Persistence::Persistent,
    );
    let started = std::time::Instant::now();

    loop {
        let output = std::process::Command::new("curl")
            .args([
                "-k",
                "-sS",
                "-w",
                "\n%{http_code}",
                "--max-time",
                "5",
                &health_url,
            ])
            .output();

        if let Ok(result) = output
            && result.status.success()
        {
            let stdout = String::from_utf8_lossy(&result.stdout).to_string();
            let mut lines = stdout.lines().collect::<Vec<_>>();
            let Some(code_line) = lines.pop() else {
                continue;
            };
            let body = lines.join("\n");
            if let Ok(code) = code_line.trim().parse::<u16>()
                && (accepted_statuses.is_empty() && (200..=299).contains(&code)
                    || accepted_statuses.contains(&code))
                && body_health_is_ok(target, &body)
            {
                output::event(
                    &target.name,
                    LogLevel::Success,
                    &format!("Health check passed at {health_url} ({code})"),
                    Persistence::Persistent,
                );
                return Ok(());
            }
        }

        if started.elapsed() >= Duration::from_secs(timeout_secs) {
            anyhow::bail!(
                "app service '{}' did not become healthy at {} within {}s",
                target.name,
                health_url,
                timeout_secs
            );
        }

        thread::sleep(Duration::from_secs(interval_secs));
    }
}
