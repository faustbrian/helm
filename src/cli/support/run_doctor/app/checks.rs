use crate::output::{self, LogLevel, Persistence};
use crate::{config, serve};

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

pub(super) fn check_octane_runtime(target: &config::ServiceConfig) -> bool {
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
