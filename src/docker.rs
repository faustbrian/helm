//! Docker container management for service instances.

#![allow(clippy::print_stdout)] // Container operations need to print status
#![allow(clippy::items_after_statements)] // Local helper imports keep function scopes tight
#![allow(clippy::match_same_arms)] // Explicit driver branches document intent
#![allow(clippy::redundant_iter_cloned)] // Thread spawn paths need owned service values

use std::sync::atomic::{AtomicBool, Ordering};

use crate::output::{self, LogLevel, Persistence};

static DRY_RUN: AtomicBool = AtomicBool::new(false);

/// Pull behavior used by `up`.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum PullPolicy {
    Always,
    Missing,
    Never,
}

/// Options for starting a container.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct UpOptions {
    pub pull: PullPolicy,
    pub recreate: bool,
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

pub(crate) fn print_docker_command(args: &[String]) {
    output::event(
        "docker",
        LogLevel::Info,
        &format!("[dry-run] docker {}", args.join(" ")),
        Persistence::Transient,
    );
}

mod exec;
mod health;
mod inspect;
mod labels;
mod logs;
mod manage;
mod ops;
mod up;

pub use exec::{exec_command, exec_interactive, exec_piped};
pub use health::wait_until_healthy;
pub use inspect::{
    inspect_env, inspect_host_port_binding, inspect_json, inspect_label, inspect_status,
};
pub(crate) use labels::{
    LABEL_CONTAINER, LABEL_KIND, LABEL_MANAGED, LABEL_SERVICE, VALUE_MANAGED_TRUE, kind_label_value,
};
pub use logs::{logs, logs_many, logs_prefixed};
pub use manage::{down, pull, recreate, restart, rm, stop};
pub use ops::{
    attach, cp, events, inspect_container, kill, pause, port, port_output, prune,
    prune_stopped_container, stats, top, unpause, wait,
};
pub use up::up;
