//! database restore process module.
//!
//! Contains database restore process logic used by Helm command workflows.

use anyhow::{Context, Result};
use std::process::Child;

/// Waits for for restore success to reach a ready state.
pub(super) fn wait_for_restore_success(child: Child) -> Result<()> {
    let output = child
        .wait_with_output()
        .context("Failed to wait for restore process")?;

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if error_msg.is_empty() {
            anyhow::bail!(
                "Database restore failed with exit code: {:?}",
                output.status.code()
            );
        }
        anyhow::bail!("Database restore failed: {error_msg}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::wait_for_restore_success;
    use std::process::{Command, Stdio};

    #[test]
    fn wait_for_restore_success_reports_non_utf8_stderr() {
        let child = Command::new("sh")
            .args(["-c", "printf '\\377' >&2; exit 1"])
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn failing command");

        let error = wait_for_restore_success(child).expect_err("expected restore failure");
        let rendered = error.to_string();

        assert!(rendered.starts_with("Database restore failed:"));
        assert_ne!(rendered.trim_end(), "Database restore failed:");
    }
}
