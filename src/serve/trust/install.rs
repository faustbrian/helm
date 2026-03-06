//! Host trust-store installation helpers.

use anyhow::Result;
use std::process::Command;

use crate::output::{self, LogLevel, Persistence};

/// Installs the extracted container CA cert into the macOS System keychain.
pub(super) fn install_cert_trust(output_path: &str, detached: bool) -> Result<()> {
    if detached {
        output::event(
            "trust",
            LogLevel::Info,
            "Skipping interactive CA trust in detached mode; run without --detached to install trust",
            Persistence::Persistent,
        );
        return Ok(());
    }

    let failure_message = format!(
        "failed to trust container CA certificate\n\
         run manually:\n\
         sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain {}",
        output_path
    );
    super::command::checked_output(
        Command::new("sudo").args([
            "security",
            "add-trusted-cert",
            "-d",
            "-r",
            "trustRoot",
            "-k",
            "/Library/Keychains/System.keychain",
            output_path,
        ]),
        "failed to execute sudo security add-trusted-cert",
        &failure_message,
    )?;

    output::event(
        "trust",
        LogLevel::Success,
        &format!("Trusted inner container CA from {output_path}"),
        Persistence::Persistent,
    );
    Ok(())
}
