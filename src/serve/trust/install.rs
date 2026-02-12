use anyhow::{Context, Result};
use std::process::Command;

use crate::output::{self, LogLevel, Persistence};

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

    let add_trust_output = Command::new("sudo")
        .args([
            "security",
            "add-trusted-cert",
            "-d",
            "-r",
            "trustRoot",
            "-k",
            "/Library/Keychains/System.keychain",
            output_path,
        ])
        .output()
        .context("failed to execute sudo security add-trusted-cert")?;

    if !add_trust_output.status.success() {
        let stderr = String::from_utf8_lossy(&add_trust_output.stderr);
        anyhow::bail!(
            "failed to trust container CA certificate: {stderr}\n\
             run manually:\n\
             sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain {output_path}"
        );
    }

    output::event(
        "trust",
        LogLevel::Success,
        &format!("Trusted inner container CA from {output_path}"),
        Persistence::Persistent,
    );
    Ok(())
}
