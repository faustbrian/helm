use super::{
    IpcEventJournal, ProjectCommandQueue, ProjectDiscoveryOptions, ProjectLogSessionRegistry,
};
use crate::control_plane::application::ControlPlane;
use crate::control_plane::daemon::ipc::IpcRequest;

/// Complete state and correlation inputs for one singleton IPC dispatch.
pub(crate) struct DaemonRequestDispatchOptions<'operation, Store> {
    pub(crate) control_plane: &'operation mut ControlPlane<Store>,
    pub(crate) discovery_options: ProjectDiscoveryOptions,
    pub(crate) request: &'operation IpcRequest,
    pub(crate) event_journal: &'operation mut IpcEventJournal,
    pub(crate) project_commands: &'operation mut ProjectCommandQueue,
    pub(crate) project_logs: &'operation mut ProjectLogSessionRegistry,
    pub(crate) now_unix_seconds: i64,
}
