use super::{
    IpcBenchmarkSnapshot, IpcDiagnostic, IpcEvent, IpcInstallationDeletionPlan,
    IpcInstallationDeletionStatus, IpcLogChunk, IpcLogSessionState, IpcManagedEnvironment,
    IpcMigrationStatus, IpcPostgresPrunePlan, IpcProjectStatus, IpcRecoveryPoint,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A typed successful daemon operation result.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub(crate) enum IpcResult {
    /// Confirms the daemon is responsive.
    Pong,
    /// Returns current daemon-wide diagnostics.
    DaemonStatus {
        engine_available: bool,
        discovery_diagnostics: Vec<IpcDiagnostic>,
    },
    /// Confirms an asynchronous operation was accepted.
    Accepted { operation_id: String },
    /// Reports one complete watched-root reconciliation attempt.
    Reconciled { project_count: usize },
    /// Confirms that exact certificate activation was scheduled.
    GatewayCertificateActivationRequested { generation: String },
    /// Returns one complete normalized sample of current owned containers.
    BenchmarkSnapshot { snapshot: IpcBenchmarkSnapshot },
    /// Returns immutable manifest references keyed by the caller's exact IDs.
    ImageReferencesResolved {
        references: BTreeMap<String, String>,
    },
    /// Confirms exact retained state was atomically reactivated.
    ProjectAdopted { project_id: String },
    /// Returns one project status derived from authoritative daemon state.
    ProjectStatus { project: IpcProjectStatus },
    /// Returns retained resources grouped by their exact former project ID.
    RetainedProjectStatus { projects: Vec<IpcProjectStatus> },
    /// Returns stable migration checkpoints without credentials or recovery paths.
    ProjectMigrations { migrations: Vec<IpcMigrationStatus> },
    /// Returns immutable verified recovery evidence newest-first.
    ProjectRecoveryPoints {
        recovery_points: Vec<IpcRecoveryPoint>,
    },
    /// Returns complete secret-free installation deletion intent.
    InstallationDeletionPlan { plan: IpcInstallationDeletionPlan },
    /// Confirms that the exact installation deletion plan was frozen.
    InstallationDeletionStarted,
    /// Returns durable installation deletion progress.
    InstallationDeletionStatus {
        status: IpcInstallationDeletionStatus,
    },
    /// Returns an exact secret-free deletion plan and confirmation token.
    PostgresPrunePlan { plan: IpcPostgresPrunePlan },
    /// Returns explicitly requested managed values over the user-only channel.
    ProjectEnvironment { environment: IpcManagedEnvironment },
    /// Returns one bounded ordered page without persisting application logs.
    ProjectLogs {
        session_id: String,
        chunks: Vec<IpcLogChunk>,
        latest_sequence: u64,
        state: IpcLogSessionState,
    },
    /// Returns the retained ordered daemon events after one optional cursor.
    Events {
        events: Vec<IpcEvent>,
        latest_sequence: u64,
    },
}
