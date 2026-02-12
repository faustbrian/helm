use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use super::targets::ResolvedSwarmTarget;
use io::spawn_prefixed_output_threads;
use wait::wait_for_target_completion;

mod io;
mod wait;

#[derive(Clone, Copy)]
pub(super) enum OutputMode {
    Logged,
    Passthrough,
}

#[derive(Debug)]
pub(super) struct SwarmRunResult {
    pub(super) name: String,
    pub(super) root: PathBuf,
    pub(super) success: bool,
    pub(super) skipped: bool,
}

pub(super) fn run_swarm_target(
    helm_executable: &Path,
    target: &ResolvedSwarmTarget,
    args: &[String],
    output_mode: OutputMode,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<SwarmRunResult> {
    let mut command = Command::new(helm_executable);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if matches!(output_mode, OutputMode::Passthrough) && !args.iter().any(|arg| arg == "--no-color")
    {
        command.env("CLICOLOR_FORCE", "1");
        command.env_remove("NO_COLOR");
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to run swarm target '{}'", target.name))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("missing stdout pipe for '{}'", target.name))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("missing stderr pipe for '{}'", target.name))?;

    let (stdout_thread, stderr_thread) =
        spawn_prefixed_output_threads(&target.name, stdout, stderr, output_mode);

    let result = wait_for_target_completion(&mut child, target, output_mode, cancel)?;

    drop(stdout_thread.join());
    drop(stderr_thread.join());

    Ok(result)
}
