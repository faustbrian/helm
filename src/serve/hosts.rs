//! `/etc/hosts` management for domain-based local serve access.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::service_domains;
use resolve::warn_if_domain_resolution_missing;
use write::append_hosts_entry_with_sudo;

mod resolve;
mod write;

const HOSTS_FILE: &str = "/etc/hosts";

/// Ensures every target domain resolves via a localhost hosts entry.
///
/// Tries direct append first, then falls back to a sudo-assisted append when
/// direct append fails with privilege-related errors.
pub(super) fn ensure_hosts_entry(target: &ServiceConfig) -> Result<()> {
    for domain in service_domains(target)? {
        if domain_resolves_to_loopback(domain) {
            continue;
        }

        if crate::docker::is_dry_run() {
            output::event(
                &target.name,
                LogLevel::Info,
                &format!("[dry-run] Ensure hosts entry: 127.0.0.1 {domain} in {HOSTS_FILE}"),
                Persistence::Transient,
            );
            continue;
        }

        let hosts_path = Path::new(HOSTS_FILE);
        let existing = std::fs::read_to_string(hosts_path)
            .with_context(|| format!("failed to read {HOSTS_FILE}"))?;

        if hosts_file_has_domain(&existing, domain) {
            continue;
        }

        let mut file = match std::fs::OpenOptions::new().append(true).open(hosts_path) {
            Ok(file) => file,
            Err(error) if should_attempt_privileged_append(&error, cfg!(target_os = "linux")) => {
                append_hosts_entry_with_sudo(domain).with_context(|| {
                    format!(
                        "failed privileged append after direct append failed for {HOSTS_FILE}: {error}"
                    )
                })?;
                emit_hosts_entry_added(&target.name, domain, true);
                continue;
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to open {HOSTS_FILE} for append"));
            }
        };

        writeln!(file, "127.0.0.1 {domain}")
            .with_context(|| format!("failed to append hosts entry to {HOSTS_FILE}"))?;

        emit_hosts_entry_added(&target.name, domain, false);
    }

    warn_if_domain_resolution_missing(target)?;
    Ok(())
}

/// Compatibility wrapper for domain hosts-entry setup.
pub(crate) fn ensure_hosts_entry_for_domain(target: &ServiceConfig) -> Result<()> {
    ensure_hosts_entry(target)
}

pub(crate) use resolve::domain_resolves_to_loopback;
pub(super) use resolve::hosts_file_has_domain;

fn emit_hosts_entry_added(target_name: &str, domain: &str, with_sudo: bool) {
    let message = if with_sudo {
        format!("Added hosts entry with sudo: 127.0.0.1 {domain}")
    } else {
        format!("Added hosts entry: 127.0.0.1 {domain}")
    };
    output::event(
        target_name,
        LogLevel::Success,
        &message,
        Persistence::Persistent,
    );
}

fn should_attempt_privileged_append(error: &std::io::Error, is_linux: bool) -> bool {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        return true;
    }
    if !is_linux {
        return false;
    }

    matches!(error.raw_os_error(), Some(30))
}

#[cfg(test)]
mod tests {
    use super::should_attempt_privileged_append;

    #[test]
    fn privileged_append_attempted_for_permission_denied() {
        let error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        assert!(should_attempt_privileged_append(&error, false));
        assert!(should_attempt_privileged_append(&error, true));
    }

    #[test]
    fn privileged_append_attempted_for_linux_read_only_filesystem() {
        let error = std::io::Error::from_raw_os_error(30);
        assert!(should_attempt_privileged_append(&error, true));
        assert!(!should_attempt_privileged_append(&error, false));
    }

    #[test]
    fn privileged_append_not_attempted_for_unrelated_open_errors() {
        let error = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
        assert!(!should_attempt_privileged_append(&error, true));
    }
}
