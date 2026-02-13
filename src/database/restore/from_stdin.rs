//! database restore from stdin module.
//!
//! Contains database restore from stdin logic used by Helm command workflows.

use anyhow::{Context, Result};
use flate2::read::GzDecoder;

use crate::config::ServiceConfig;

use super::process::wait_for_restore_success;

pub(super) fn restore_from_stdin(service: &ServiceConfig, gzip: bool) -> Result<()> {
    let mut child =
        crate::docker::exec_piped(service, false).context("Failed to start restore process")?;
    let mut stdin_pipe = child.stdin.take().context("Failed to open stdin pipe")?;

    if gzip {
        let stdin = std::io::stdin();
        let mut decoder = GzDecoder::new(stdin.lock());
        std::io::copy(&mut decoder, &mut stdin_pipe)
            .context("Failed to pipe gzip stdin to database process")?;
    } else {
        std::io::copy(&mut std::io::stdin().lock(), &mut stdin_pipe)
            .context("Failed to pipe stdin to database process")?;
    }

    drop(stdin_pipe);
    wait_for_restore_success(child)
}
