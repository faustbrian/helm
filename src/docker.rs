//! Docker container management for service instances.

#![allow(clippy::print_stdout)] // Container operations need to print status
#![allow(clippy::items_after_statements)] // Local helper imports keep function scopes tight
#![allow(clippy::match_same_arms)] // Explicit driver branches document intent
#![allow(clippy::redundant_iter_cloned)] // Thread spawn paths need owned service values

#[cfg(test)]
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::output::{self, LogLevel, Persistence};

static DRY_RUN: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static TEST_DOCKER_SCOPE_LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();
#[cfg(test)]
static TEST_DOCKER_COMMAND: std::sync::OnceLock<Mutex<Option<String>>> = std::sync::OnceLock::new();
#[cfg(test)]
static TEST_DRY_RUN: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();

/// Pull behavior used by `up`.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum PullPolicy {
    Always,
    Missing,
    Never,
}

/// Enable or disable dry-run mode.
pub fn set_dry_run(enabled: bool) {
    DRY_RUN.store(enabled, Ordering::SeqCst);
}

/// Returns true when dry-run mode is enabled.
#[must_use]
pub fn is_dry_run() -> bool {
    DRY_RUN.load(Ordering::SeqCst)
}

#[cfg(test)]
pub(crate) fn with_dry_run_lock<R>(test: impl FnOnce() -> R) -> R {
    with_dry_run_state(true, test)
}

#[cfg(test)]
pub(crate) fn with_dry_run_state<R>(enabled: bool, test: impl FnOnce() -> R) -> R {
    let guard = TEST_DRY_RUN.get_or_init(Default::default).lock();
    let guard = match guard {
        Ok(guard) => guard,
        Err(err) => err.into_inner(),
    };

    let previous = DRY_RUN.load(Ordering::SeqCst);
    DRY_RUN.store(enabled, Ordering::SeqCst);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(test));

    DRY_RUN.store(previous, Ordering::SeqCst);
    drop(guard);

    match result {
        Ok(result) => result,
        Err(err) => std::panic::resume_unwind(err),
    }
}

pub(crate) fn print_docker_command(args: &[String]) {
    output::event(
        "docker",
        LogLevel::Info,
        &format!("[dry-run] docker {}", args.join(" ")),
        Persistence::Transient,
    );
}

#[cfg(test)]
pub(crate) fn with_docker_command<F, T>(command: &str, test: F) -> T
where
    F: FnOnce() -> T,
{
    let scope_lock = TEST_DOCKER_SCOPE_LOCK.get_or_init(Default::default).lock();
    let _scope_lock = match scope_lock {
        Ok(lock) => lock,
        Err(err) => err.into_inner(),
    };

    let previous = {
        let mut command_state = TEST_DOCKER_COMMAND
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        let previous = command_state.clone();
        *command_state = Some(command.to_owned());
        previous
    };

    let result = {
        let test = std::panic::AssertUnwindSafe(test);
        std::panic::catch_unwind(test)
    };

    {
        let mut command_state = TEST_DOCKER_COMMAND
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        *command_state = previous;
    }
    match result {
        Ok(result) => result,
        Err(err) => std::panic::resume_unwind(err),
    }
}

pub(crate) fn docker_command() -> String {
    #[cfg(test)]
    {
        let command_state = TEST_DOCKER_COMMAND
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        if let Some(command) = command_state.clone() {
            return command;
        }
    }
    "docker".to_owned()
}

mod cmd;
mod exec;
mod health;
mod image_inspect;
mod image_pull;
mod inspect;
mod labels;
mod logs;
mod manage;
mod ops;
mod policy;
mod scheduler;
mod up;

pub(crate) use cmd::{
    docker_arg_refs, ensure_docker_output_success, run_docker_output, run_docker_output_owned,
    run_docker_status, run_docker_status_owned, spawn_docker_stdin_stderr_piped,
    spawn_docker_stdout_stderr_piped,
};
pub(crate) use exec::build_exec_args;
pub use exec::{exec_command, exec_interactive, exec_piped};
pub use health::wait_until_healthy;
pub(crate) use image_inspect::{docker_image_exists, docker_image_repo_digest};
pub(crate) use image_pull::docker_pull;
pub use inspect::{
    inspect_env, inspect_host_port_binding, inspect_json, inspect_label, inspect_status,
};
pub(crate) use labels::{
    LABEL_CONTAINER, LABEL_KIND, LABEL_MANAGED, LABEL_SERVICE, VALUE_MANAGED_TRUE, kind_label_value,
};
pub use logs::{LogsOptions, logs, logs_many, logs_prefixed};
pub use manage::{down, pull, recreate, restart, rm, stop};
pub use ops::{
    CpOptions, PruneOptions, StatsOptions, attach, cp, events, inspect_container, kill, pause,
    port, port_output, prune, prune_stopped_container, stats, top, unpause, wait,
};
pub(crate) use policy::{DockerPolicyOverrides, set_policy_overrides};
pub(crate) use scheduler::{DockerOpClass, with_scheduled_docker_op};
pub use up::up;

#[cfg(test)]
mod tests {
    use crate::docker::{is_dry_run, with_dry_run_lock};

    #[test]
    fn with_dry_run_lock_sets_dry_run_inside_closure() {
        with_dry_run_lock(|| {
            assert!(is_dry_run());
        });
    }

    #[test]
    fn with_dry_run_lock_propagates_panics() {
        let result = std::panic::catch_unwind(|| {
            with_dry_run_lock(|| {
                panic!("simulated panic");
            })
        });

        assert!(result.is_err());
    }
}
