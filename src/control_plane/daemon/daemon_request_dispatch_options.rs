use super::{
    BenchmarkSnapshotProvider, ImageReferenceResolution, IpcEventJournal, MigrationDecisionQueue,
    PostgresPruneQueue, ProjectBackupQueue, ProjectCommandQueue, ProjectDiscoveryOptions,
    ProjectLogSessionRegistry, ProjectRestoreQueue, ResourceHealthRegistry,
    V7ProjectInventoryProvider,
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
    pub(crate) project_backups: &'operation mut ProjectBackupQueue,
    pub(crate) postgres_prunes: &'operation mut PostgresPruneQueue,
    pub(crate) project_restores: &'operation mut ProjectRestoreQueue,
    pub(crate) migration_decisions: &'operation mut MigrationDecisionQueue,
    pub(crate) project_logs: &'operation mut ProjectLogSessionRegistry,
    pub(crate) resource_health: &'operation ResourceHealthRegistry,
    pub(crate) benchmark_snapshot: Option<&'operation mut dyn BenchmarkSnapshotProvider>,
    pub(crate) image_reference_resolution: Option<&'operation mut dyn ImageReferenceResolution>,
    pub(crate) v7_project_inventory: Option<&'operation mut dyn V7ProjectInventoryProvider>,
    pub(crate) now_unix_seconds: i64,
}
