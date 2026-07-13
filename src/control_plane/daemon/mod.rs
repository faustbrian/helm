mod daemon_iteration_result;
#[cfg(unix)]
mod default_unix_daemon_runtime_directory;
mod discover_project_sources;
mod discovery_reconciliation_error;
mod discovery_reconciliation_result;
mod discovery_scan_reason;
mod discovery_scheduler;
mod discovery_scheduler_error;
mod discovery_scheduler_options;
mod dispatch_daemon_request;
mod filesystem_event_watcher;
mod filesystem_event_watcher_error;
mod ipc;
mod project_discovery_error;
mod project_discovery_issue;
mod project_discovery_options;
mod project_discovery_report;
mod reconcile_watched_roots;
mod retry_backoff;
mod retry_backoff_error;
mod retry_backoff_options;
mod retry_delay;
#[cfg(unix)]
mod run_unix_daemon_watch;
mod singleton_lease;
mod singleton_lease_error;
#[cfg(unix)]
mod unix_daemon_runtime;
#[cfg(unix)]
mod unix_daemon_runtime_error;
#[cfg(unix)]
mod unix_daemon_runtime_options;
#[cfg(unix)]
mod unix_daemon_watch_options;

pub(crate) use singleton_lease::SingletonLease;
pub(crate) use singleton_lease_error::SingletonLeaseError;

#[cfg(test)]
mod tests;
pub(crate) use daemon_iteration_result::DaemonIterationResult;
#[cfg(unix)]
pub(crate) use default_unix_daemon_runtime_directory::default_unix_daemon_runtime_directory;
pub(crate) use discover_project_sources::discover_project_sources;
pub(crate) use discovery_reconciliation_error::DiscoveryReconciliationError;
pub(crate) use discovery_reconciliation_result::DiscoveryReconciliationResult;
pub(crate) use discovery_scan_reason::DiscoveryScanReason;
pub(crate) use discovery_scheduler::DiscoveryScheduler;
pub(crate) use discovery_scheduler_error::DiscoverySchedulerError;
pub(crate) use discovery_scheduler_options::DiscoverySchedulerOptions;
pub(crate) use dispatch_daemon_request::dispatch_daemon_request;
pub(crate) use filesystem_event_watcher::FilesystemEventWatcher;
pub(crate) use filesystem_event_watcher_error::FilesystemEventWatcherError;
#[cfg(unix)]
pub(crate) use ipc::{
    IpcOutcome, IpcPayload, IpcRequest, IpcResponse, IpcResult, send_unix_request,
};
pub(crate) use project_discovery_error::ProjectDiscoveryError;
pub(crate) use project_discovery_issue::ProjectDiscoveryIssue;
pub(crate) use project_discovery_options::ProjectDiscoveryOptions;
pub(crate) use project_discovery_report::ProjectDiscoveryReport;
pub(crate) use reconcile_watched_roots::reconcile_watched_roots;
pub(crate) use retry_backoff::RetryBackoff;
pub(crate) use retry_backoff_error::RetryBackoffError;
pub(crate) use retry_backoff_options::RetryBackoffOptions;
pub(crate) use retry_delay::RetryDelay;
#[cfg(unix)]
pub(crate) use run_unix_daemon_watch::run_unix_daemon_watch;
#[cfg(unix)]
pub(crate) use unix_daemon_runtime::UnixDaemonRuntime;
#[cfg(unix)]
pub(crate) use unix_daemon_runtime_error::UnixDaemonRuntimeError;
#[cfg(unix)]
pub(crate) use unix_daemon_runtime_options::UnixDaemonRuntimeOptions;
#[cfg(unix)]
pub(crate) use unix_daemon_watch_options::UnixDaemonWatchOptions;
