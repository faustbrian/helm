//! cli support run doctor app checks module.
//!
//! Contains cli support run doctor app checks logic used by Helm command workflows.

use crate::output::{self, LogLevel, Persistence};
use crate::{cli, config, docker, serve};

/// Checks domain resolution and reports actionable failures.
pub(super) fn check_domain_resolution(target: &config::ServiceConfig, fix: bool) -> bool {
    let domains = target.resolved_domains();
    if domains.is_empty() || target.localhost_tls {
        return false;
    }

    if fix {
        if let Err(err) = serve::ensure_hosts_entry_for_domain(target) {
            output::event(
                "doctor",
                LogLevel::Error,
                &format!("Hosts fix failed for '{}': {}", target.name, err),
                Persistence::Persistent,
            );
            return true;
        }
        output::event(
            "doctor",
            LogLevel::Success,
            &format!("Hosts entry ensured for {}", domains.join(", ")),
            Persistence::Persistent,
        );
        return false;
    }

    let mut has_error = false;
    for domain in domains {
        if !serve::domain_resolves_to_loopback(domain) {
            output::event(
                "doctor",
                LogLevel::Error,
                &format!("{domain} does not resolve to localhost"),
                Persistence::Persistent,
            );
            has_error = true;
            continue;
        }

        output::event(
            "doctor",
            LogLevel::Success,
            &format!("{domain} resolves to localhost"),
            Persistence::Persistent,
        );
    }
    has_error
}

/// Checks php extensions and reports actionable failures.
pub(super) fn check_php_extensions(target: &config::ServiceConfig) -> bool {
    match serve::verify_php_extensions(target) {
        Ok(Some(check)) if check.missing.is_empty() => {
            output::event(
                "doctor",
                LogLevel::Success,
                &format!(
                    "App service '{}' extensions available in {}",
                    check.target, check.image
                ),
                Persistence::Persistent,
            );
            false
        }
        Ok(Some(check)) => {
            output::event(
                "doctor",
                LogLevel::Error,
                &format!(
                    "App service '{}' missing PHP extensions in {}: {}",
                    check.target,
                    check.image,
                    check.missing.join(", ")
                ),
                Persistence::Persistent,
            );
            true
        }
        Ok(None) => false,
        Err(err) => {
            output::event(
                "doctor",
                LogLevel::Error,
                &format!(
                    "App service '{}' extension verification failed: {}",
                    target.name, err
                ),
                Persistence::Persistent,
            );
            true
        }
    }
}

/// Checks octane runtime and reports actionable failures.
pub(super) fn check_octane_runtime(
    target: &config::ServiceConfig,
    allow_stopped_runtime_checks: bool,
) -> bool {
    match serve::runtime_cmdline(target) {
        Ok(Some(cmdline)) if cmdline.contains("octane:frankenphp") => {
            output::event(
                "doctor",
                LogLevel::Success,
                &format!("App service '{}' running with octane", target.name),
                Persistence::Persistent,
            );
            false
        }
        Ok(Some(cmdline)) => {
            output::event(
                "doctor",
                LogLevel::Error,
                &format!(
                    "App service '{}' not running octane (pid1: {})",
                    target.name, cmdline
                ),
                Persistence::Persistent,
            );
            true
        }
        Ok(None) => {
            if allow_stopped_runtime_checks {
                output::event(
                    "doctor",
                    LogLevel::Info,
                    &format!(
                        "App service '{}' is not running; octane runtime check deferred",
                        target.name
                    ),
                    Persistence::Persistent,
                );
                return false;
            }
            output::event(
                "doctor",
                LogLevel::Error,
                &format!(
                    "App service '{}' is not running for octane check",
                    target.name
                ),
                Persistence::Persistent,
            );
            true
        }
        Err(err) => {
            output::event(
                "doctor",
                LogLevel::Error,
                &format!("App service '{}' octane check failed: {}", target.name, err),
                Persistence::Persistent,
            );
            true
        }
    }
}

/// Checks public app and health endpoint reachability for a serve target.
pub(super) fn check_http_reachability(target: &config::ServiceConfig) -> bool {
    let mut has_error = false;

    let status = target
        .container_name()
        .ok()
        .and_then(|name| docker::inspect_status(&name))
        .unwrap_or_else(|| "not created".to_owned());
    if status != "running" {
        output::event(
            "doctor",
            LogLevel::Error,
            &format!(
                "App service '{}' is not running (status: {status})",
                target.name
            ),
            Persistence::Persistent,
        );
        return true;
    }

    let app_url = match serve::public_url(target) {
        Ok(url) => url,
        Err(err) => {
            output::event(
                "doctor",
                LogLevel::Error,
                &format!("App service '{}' URL resolution failed: {err}", target.name),
                Persistence::Persistent,
            );
            return true;
        }
    };

    let health_path = target.health_path.as_deref().unwrap_or("/up");
    let health_path = if health_path.starts_with('/') {
        health_path.to_owned()
    } else {
        format!("/{health_path}")
    };
    let health_url = format!("{}{}", app_url.trim_end_matches('/'), health_path);

    if let Some(status_code) = cli::support::probe_http_status(&app_url) {
        if (200..400).contains(&status_code) {
            output::event(
                "doctor",
                LogLevel::Success,
                &format!("{} app URL reachable ({status_code})", target.name),
                Persistence::Persistent,
            );
        } else {
            has_error = true;
            output::event(
                "doctor",
                LogLevel::Error,
                &format!(
                    "{} app URL unhealthy ({status_code}): {app_url}",
                    target.name
                ),
                Persistence::Persistent,
            );
        }
    } else {
        has_error = true;
        output::event(
            "doctor",
            LogLevel::Error,
            &format!("{} app URL unreachable: {app_url}", target.name),
            Persistence::Persistent,
        );
    }

    if let Some(status_code) = cli::support::probe_http_status(&health_url) {
        let accepted = target
            .health_statuses
            .as_ref()
            .is_some_and(|values| values.contains(&status_code))
            || (200..300).contains(&status_code);
        if accepted {
            output::event(
                "doctor",
                LogLevel::Success,
                &format!("{} health endpoint reachable ({status_code})", target.name),
                Persistence::Persistent,
            );
        } else {
            has_error = true;
            output::event(
                "doctor",
                LogLevel::Error,
                &format!(
                    "{} health endpoint returned {status_code}: {health_url}",
                    target.name
                ),
                Persistence::Persistent,
            );
        }
    } else {
        has_error = true;
        output::event(
            "doctor",
            LogLevel::Error,
            &format!("{} health endpoint unreachable: {health_url}", target.name),
            Persistence::Persistent,
        );
    }

    has_error
}
