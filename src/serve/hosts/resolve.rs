//! Hostname resolution and hosts-file parsing helpers.

use anyhow::Result;
use std::net::ToSocketAddrs;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::super::service_domains;

/// Returns whether the domain currently resolves to a loopback address.
pub(crate) fn domain_resolves_to_loopback(domain: &str) -> bool {
    let addr_text = format!("{domain}:443");
    if let Ok(mut addrs) = addr_text.to_socket_addrs() {
        return addrs.any(|addr| addr.ip().is_loopback());
    }
    false
}

/// Returns whether an `/etc/hosts`-style file already maps the given domain.
pub(crate) fn hosts_file_has_domain(content: &str, domain: &str) -> bool {
    content.lines().any(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return false;
        }

        let no_comment = trimmed.split('#').next().unwrap_or("").trim();
        no_comment
            .split_whitespace()
            .skip(1)
            .any(|entry| entry == domain)
    })
}

/// Emits warnings for domains that still do not resolve to loopback.
pub(super) fn warn_if_domain_resolution_missing(target: &ServiceConfig) -> Result<()> {
    for domain in service_domains(target)? {
        if domain_resolves_to_loopback(domain) {
            continue;
        }

        output::event(
            &target.name,
            LogLevel::Warn,
            &format!(
                "{domain} does not currently resolve to localhost. Add a hosts or DNS entry for local HTTPS access"
            ),
            Persistence::Persistent,
        );
    }
    Ok(())
}
