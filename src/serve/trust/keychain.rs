//! System keychain inspection helpers for trust flow.

use anyhow::Result;
use std::process::Command;

/// Returns whether a SHA256 certificate fingerprint exists in System keychain.
pub(super) fn is_cert_trusted_in_system_keychain(fingerprint: &str) -> Result<bool> {
    let output = super::command::checked_output(
        Command::new("security").args([
            "find-certificate",
            "-a",
            "-Z",
            "/Library/Keychains/System.keychain",
        ]),
        "failed to query system keychain certificates",
        "failed to inspect system keychain",
    )?;

    let haystack = String::from_utf8_lossy(&output.stdout).to_uppercase();
    Ok(haystack.contains(fingerprint))
}
