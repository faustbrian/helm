//! Privileged hosts-file write helpers.

use anyhow::{Context, Result};
use std::process::Command;

use super::HOSTS_FILE;

/// Appends a hosts entry using `sudo` when direct file append is unavailable.
pub(super) fn append_hosts_entry_with_sudo(domain: &str) -> Result<()> {
    validate_hostname(domain)?;
    let command = format!("echo '127.0.0.1 {domain}' >> {HOSTS_FILE}");
    let status = Command::new("sudo")
        .args(["sh", "-c", &command])
        .status()
        .context("failed to run sudo for hosts entry update")?;

    if status.success() {
        return Ok(());
    }

    anyhow::bail!(
        "could not update {HOSTS_FILE} for domain '{domain}'.\n\
         run manually:\n\
           echo '127.0.0.1 {domain}' | sudo tee -a {HOSTS_FILE}"
    );
}

/// Validates hostnames accepted for hosts-file entry generation.
fn validate_hostname(hostname: &str) -> Result<()> {
    let valid = !hostname.is_empty()
        && hostname
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '.' || ch == '-');
    if valid {
        return Ok(());
    }
    anyhow::bail!("invalid hostname '{hostname}'");
}
