use anyhow::{Context, Result};
use std::io::Read;
use std::process::Child;

pub(super) fn wait_for_restore_success(mut child: Child) -> Result<()> {
    let status = child.wait().context("Failed to wait for restore process")?;

    if !status.success() {
        if let Some(mut stderr) = child.stderr {
            let mut error_msg = String::new();
            drop(stderr.read_to_string(&mut error_msg));
            anyhow::bail!("Database restore failed: {error_msg}");
        }
        anyhow::bail!(
            "Database restore failed with exit code: {:?}",
            status.code()
        );
    }

    Ok(())
}
