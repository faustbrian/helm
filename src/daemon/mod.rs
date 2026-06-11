//! Per-project daemon state and process helpers.

mod discovery;
mod process;
mod service;
mod state;
mod supervisor;

pub(crate) use discovery::{DiscoveryOptions, DiscoveryReport, discover_projects};
pub(crate) use process::{daemon_binary, pid_is_running, spawn_detached, stop_pid};
pub(crate) use service::{
    DaemonServiceInstallOptions, ServiceManager, install_service, print_service, service_status,
    uninstall_service,
};
pub(crate) use state::{
    DaemonSession, clear_session, daemon_log_path, load_session, now_unix, save_session,
};
pub(crate) use supervisor::run as run_supervisor;

#[cfg(test)]
pub(crate) use process::{clear_test_daemon_binary, set_test_daemon_binary};
#[cfg(test)]
pub(crate) use state::{clear_test_daemon_home, set_test_daemon_home};
