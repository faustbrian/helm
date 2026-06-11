//! Per-project daemon state and process helpers.

mod process;
mod state;

pub(crate) use process::{pid_is_running, spawn_detached, stop_pid};
pub(crate) use state::{
    DaemonSession, clear_session, daemon_log_path, load_session, now_unix, save_session,
};

#[cfg(test)]
pub(crate) use process::{clear_test_daemon_binary, set_test_daemon_binary};
#[cfg(test)]
pub(crate) use state::{clear_test_daemon_home, set_test_daemon_home};
