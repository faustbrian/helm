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
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                append_hosts_entry_with_sudo(domain)?;
                output::event(
                    &target.name,
                    LogLevel::Success,
                    &format!("Added hosts entry with sudo: 127.0.0.1 {domain}"),
                    Persistence::Persistent,
                );
                continue;
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to open {HOSTS_FILE} for append"));
            }
        };

        writeln!(file, "127.0.0.1 {domain}")
            .with_context(|| format!("failed to append hosts entry to {HOSTS_FILE}"))?;

        output::event(
            &target.name,
            LogLevel::Success,
            &format!("Added hosts entry: 127.0.0.1 {domain}"),
            Persistence::Persistent,
        );
    }

    warn_if_domain_resolution_missing(target)?;
    Ok(())
}

pub(crate) fn ensure_hosts_entry_for_domain(target: &ServiceConfig) -> Result<()> {
    ensure_hosts_entry(target)
}

pub(crate) use resolve::domain_resolves_to_loopback;
pub(super) use resolve::hosts_file_has_domain;
