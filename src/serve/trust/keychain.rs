use anyhow::{Context, Result};
use std::process::Command;

pub(super) fn is_cert_trusted_in_system_keychain(fingerprint: &str) -> Result<bool> {
    let output = Command::new("security")
        .args([
            "find-certificate",
            "-a",
            "-Z",
            "/Library/Keychains/System.keychain",
        ])
        .output()
        .context("failed to query system keychain certificates")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("failed to inspect system keychain: {stderr}");
    }

    let haystack = String::from_utf8_lossy(&output.stdout).to_uppercase();
    Ok(haystack.contains(fingerprint))
}
