//! Certificate fingerprint extraction helpers.

use anyhow::Result;
use std::process::Command;

/// Reads and normalizes the SHA256 fingerprint for a certificate file.
pub(super) fn cert_fingerprint_sha256(cert_path: &str) -> Result<String> {
    let output = super::command::checked_output(
        Command::new("openssl").args([
            "x509",
            "-in",
            cert_path,
            "-noout",
            "-fingerprint",
            "-sha256",
        ]),
        "failed to execute openssl x509 for certificate fingerprint",
        "failed to read certificate fingerprint",
    )?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let raw = stdout
        .split('=')
        .nth(1)
        .map(str::trim)
        .ok_or_else(|| anyhow::anyhow!("unexpected openssl fingerprint output"))?;
    Ok(raw.replace(':', "").to_uppercase())
}
