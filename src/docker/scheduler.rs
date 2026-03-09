//! Lightweight Docker operation scheduler for heavy workflows.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

const SLOT_WAIT_POLL_INTERVAL: Duration = Duration::from_millis(25);
const HEAVY_SLOT_WAIT_ATTEMPTS: usize = 240;
const BUILD_SLOT_WAIT_ATTEMPTS: usize = 7_200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DockerOpClass {
    Heavy,
    Build,
}

pub(crate) fn with_scheduled_docker_op<T, F>(
    class: DockerOpClass,
    op_name: &str,
    operation: F,
) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    if crate::docker::is_dry_run() {
        return operation();
    }

    let limit = concurrency_limit(class).max(1);
    let lock_root = lock_root(class);
    std::fs::create_dir_all(&lock_root)
        .with_context(|| format!("failed to create {}", lock_root.display()))?;
    let _slot = acquire_slot(&lock_root, limit, op_name, class)?;
    operation()
}

fn concurrency_limit(class: DockerOpClass) -> usize {
    let policy = super::policy::docker_policy();
    match class {
        DockerOpClass::Heavy => policy.max_heavy_ops,
        DockerOpClass::Build => policy.max_build_ops.min(policy.max_heavy_ops),
    }
}

fn lock_root(class: DockerOpClass) -> PathBuf {
    let class_name = match class {
        DockerOpClass::Heavy => "heavy",
        DockerOpClass::Build => "build",
    };
    std::env::temp_dir()
        .join("helm-docker-scheduler")
        .join(class_name)
}

struct SlotGuard {
    path: PathBuf,
}

impl Drop for SlotGuard {
    fn drop(&mut self) {
        drop(std::fs::remove_file(&self.path));
    }
}

fn acquire_slot(
    root: &Path,
    limit: usize,
    op_name: &str,
    class: DockerOpClass,
) -> Result<SlotGuard> {
    let max_attempts = match class {
        DockerOpClass::Heavy => HEAVY_SLOT_WAIT_ATTEMPTS,
        DockerOpClass::Build => BUILD_SLOT_WAIT_ATTEMPTS,
    };
    for _ in 0..max_attempts {
        for slot in 0..limit {
            let slot_path = root.join(format!("slot-{slot}.lock"));
            match std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&slot_path)
            {
                Ok(mut file) => {
                    if writeln!(file, "{}:{op_name}", std::process::id()).is_err() {
                        // Best-effort metadata write; lock acquisition is what matters.
                    }
                    return Ok(SlotGuard { path: slot_path });
                }
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(err) => {
                    return Err(err)
                        .with_context(|| format!("failed to acquire {}", slot_path.display()));
                }
            }
        }
        std::thread::sleep(SLOT_WAIT_POLL_INTERVAL);
    }

    let wait_secs = SLOT_WAIT_POLL_INTERVAL.as_secs_f64() * max_attempts as f64;
    anyhow::bail!(
        "timed out waiting for runtime operation slot for '{op_name}' after {} attempts (~{wait_secs:.1}s)",
        max_attempts,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        BUILD_SLOT_WAIT_ATTEMPTS, DockerOpClass, SLOT_WAIT_POLL_INTERVAL, with_scheduled_docker_op,
    };
    use std::time::Duration;

    #[test]
    fn scheduled_op_runs_and_returns_result() {
        let value = with_scheduled_docker_op(DockerOpClass::Heavy, "test-op", || {
            Ok::<_, anyhow::Error>(7)
        })
        .expect("scheduled op result");
        assert_eq!(value, 7);
    }

    #[test]
    fn build_slot_timeout_budget_exceeds_long_image_builds() {
        let wait_budget = SLOT_WAIT_POLL_INTERVAL.saturating_mul(BUILD_SLOT_WAIT_ATTEMPTS as u32);
        assert!(
            wait_budget >= Duration::from_secs(60),
            "build slot wait budget should cover long derived image builds"
        );
    }
}
