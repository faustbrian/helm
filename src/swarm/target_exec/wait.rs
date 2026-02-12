use anyhow::{Context, Result};
use std::process::Child;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::output::{self, LogLevel, Persistence};
use crate::swarm::targets::ResolvedSwarmTarget;

use super::{OutputMode, SwarmRunResult};

pub(super) fn wait_for_target_completion(
    child: &mut Child,
    target: &ResolvedSwarmTarget,
    output_mode: OutputMode,
    cancel: Option<Arc<AtomicBool>>,
) -> Result<SwarmRunResult> {
    let status = loop {
        if let Some(cancel_flag) = &cancel
            && cancel_flag.load(Ordering::Relaxed)
        {
            drop(child.kill());
            let _ = child.wait().with_context(|| {
                format!("failed waiting on cancelled swarm target '{}'", target.name)
            })?;
            match output_mode {
                OutputMode::Logged => output::event(
                    &target.name,
                    LogLevel::Warn,
                    "Target cancelled due to fail-fast policy",
                    Persistence::Persistent,
                ),
                OutputMode::Passthrough => {
                    eprintln!("Target '{}' cancelled due to fail-fast policy", target.name);
                }
            }
            return Ok(SwarmRunResult {
                name: target.name.clone(),
                root: target.root.clone(),
                success: false,
                skipped: true,
            });
        }

        if let Some(done) = child
            .try_wait()
            .with_context(|| format!("failed polling swarm target '{}'", target.name))?
        {
            break done;
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
    };

    match output_mode {
        OutputMode::Logged => {
            if status.success() {
                output::event(
                    &target.name,
                    LogLevel::Success,
                    "Target completed successfully",
                    Persistence::Persistent,
                );
            } else {
                output::event(
                    &target.name,
                    LogLevel::Error,
                    "Target failed",
                    Persistence::Persistent,
                );
            }
        }
        OutputMode::Passthrough => {
            if !status.success() {
                eprintln!("Target '{}' failed", target.name);
            }
        }
    }

    Ok(SwarmRunResult {
        name: target.name.clone(),
        root: target.root.clone(),
        success: status.success(),
        skipped: false,
    })
}
