//! Strict v8 CLI handlers.

mod config_schema_cmd;
mod config_validate_cmd;
mod daemon_cmd;
mod log;
mod setup_cmd;
#[cfg(unix)]
mod v8_env_cmd;
#[cfg(unix)]
mod v8_lock_cmd;
#[cfg(unix)]
mod v8_logs_cmd;
#[cfg(unix)]
mod v8_open_cmd;
#[cfg(unix)]
mod v8_project;
#[cfg(unix)]
mod v8_project_command;
#[cfg(unix)]
mod v8_project_status;
#[cfg(unix)]
mod v8_status_cmd;
#[cfg(unix)]
mod v8_url_cmd;
#[cfg(unix)]
mod v8_workflow_cmd;

pub(crate) use config_schema_cmd::handle_config_schema;
pub(crate) use config_validate_cmd::handle_config_validate;
pub(crate) use daemon_cmd::handle_daemon;
pub(crate) use setup_cmd::handle_setup;
#[cfg(unix)]
pub(crate) use v8_env_cmd::handle_v8_env;
#[cfg(unix)]
pub(crate) use v8_lock_cmd::handle_v8_lock;
#[cfg(unix)]
pub(crate) use v8_logs_cmd::handle_v8_logs;
#[cfg(unix)]
pub(crate) use v8_open_cmd::handle_v8_open;
#[cfg(unix)]
pub(crate) use v8_project_command::handle_v8_project_command;
#[cfg(unix)]
pub(crate) use v8_status_cmd::handle_v8_status;
#[cfg(unix)]
pub(crate) use v8_url_cmd::handle_v8_url;
#[cfg(unix)]
pub(crate) use v8_workflow_cmd::handle_v8_workflow;
