use super::{
    DiscoveryReconciliationError, DiscoverySchedulerError, EngineConnectionSupervisorError,
    FilesystemEventWatcherError, InstallationInitializationError, RetryBackoffError,
    SingletonLeaseError,
};
use crate::control_plane::daemon::ipc::IpcError;
use crate::control_plane::engine::EngineError;
use crate::control_plane::state::StateStoreError;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;

/// A fatal setup or bounded-iteration Unix daemon failure.
#[derive(Debug)]
pub(crate) enum UnixDaemonRuntimeError {
    InvalidOptions {
        detail: String,
    },
    FileSystem {
        action: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
    Lease(SingletonLeaseError),
    State(StateStoreError),
    Ipc(IpcError),
    Watcher(FilesystemEventWatcherError),
    Installation(InstallationInitializationError),
    EngineConfiguration(EngineError),
    AsyncRuntime(std::io::Error),
    EngineSupervisor(EngineConnectionSupervisorError),
    Reconciliation(DiscoveryReconciliationError),
    Scheduler(DiscoverySchedulerError),
    Retry(RetryBackoffError),
}

impl Display for UnixDaemonRuntimeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOptions { detail } => formatter.write_str(detail),
            Self::FileSystem {
                action,
                path,
                source,
            } => write!(
                formatter,
                "failed to {action} '{}': {source}",
                path.display()
            ),
            Self::Lease(error) => Display::fmt(error, formatter),
            Self::State(error) => Display::fmt(error, formatter),
            Self::Ipc(error) => Display::fmt(error, formatter),
            Self::Watcher(error) => Display::fmt(error, formatter),
            Self::Installation(error) => Display::fmt(error, formatter),
            Self::EngineConfiguration(error) => {
                write!(formatter, "invalid global Engine configuration: {error}")
            }
            Self::AsyncRuntime(error) => {
                write!(formatter, "failed to create Engine async runtime: {error}")
            }
            Self::EngineSupervisor(error) => Display::fmt(error, formatter),
            Self::Reconciliation(error) => Display::fmt(error, formatter),
            Self::Scheduler(error) => Display::fmt(error, formatter),
            Self::Retry(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for UnixDaemonRuntimeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidOptions { .. } => None,
            Self::FileSystem { source, .. } => Some(source),
            Self::Lease(error) => Some(error),
            Self::State(error) => Some(error),
            Self::Ipc(error) => Some(error),
            Self::Watcher(error) => Some(error),
            Self::Installation(error) => Some(error),
            Self::EngineConfiguration(error) => Some(error),
            Self::AsyncRuntime(error) => Some(error),
            Self::EngineSupervisor(error) => Some(error),
            Self::Reconciliation(error) => Some(error),
            Self::Scheduler(error) => Some(error),
            Self::Retry(error) => Some(error),
        }
    }
}

impl From<SingletonLeaseError> for UnixDaemonRuntimeError {
    fn from(error: SingletonLeaseError) -> Self {
        Self::Lease(error)
    }
}

impl From<StateStoreError> for UnixDaemonRuntimeError {
    fn from(error: StateStoreError) -> Self {
        Self::State(error)
    }
}

impl From<IpcError> for UnixDaemonRuntimeError {
    fn from(error: IpcError) -> Self {
        Self::Ipc(error)
    }
}

impl From<FilesystemEventWatcherError> for UnixDaemonRuntimeError {
    fn from(error: FilesystemEventWatcherError) -> Self {
        Self::Watcher(error)
    }
}

impl From<InstallationInitializationError> for UnixDaemonRuntimeError {
    fn from(error: InstallationInitializationError) -> Self {
        Self::Installation(error)
    }
}

impl From<EngineError> for UnixDaemonRuntimeError {
    fn from(error: EngineError) -> Self {
        Self::EngineConfiguration(error)
    }
}

impl From<EngineConnectionSupervisorError> for UnixDaemonRuntimeError {
    fn from(error: EngineConnectionSupervisorError) -> Self {
        Self::EngineSupervisor(error)
    }
}

impl From<DiscoveryReconciliationError> for UnixDaemonRuntimeError {
    fn from(error: DiscoveryReconciliationError) -> Self {
        Self::Reconciliation(error)
    }
}

impl From<DiscoverySchedulerError> for UnixDaemonRuntimeError {
    fn from(error: DiscoverySchedulerError) -> Self {
        Self::Scheduler(error)
    }
}

impl From<RetryBackoffError> for UnixDaemonRuntimeError {
    fn from(error: RetryBackoffError) -> Self {
        Self::Retry(error)
    }
}
