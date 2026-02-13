//! cli support probe http status module.
//!
//! Contains cli support probe http status logic used by Helm command workflows.

use std::process::Command;

pub(crate) fn probe_http_status(url: &str) -> Option<u16> {
    let output = Command::new("curl")
        .args([
            "-k",
            "-sS",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "--max-time",
            "5",
            url,
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let raw = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    raw.parse::<u16>().ok()
}
