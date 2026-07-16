use super::{IpcMigrationDecision, IpcProjectCommand};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// A typed operation sent to the v8 daemon.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub(crate) enum IpcPayload {
    /// Verifies daemon availability and protocol compatibility.
    Ping,
    /// Reads current daemon-wide diagnostics without mutating state.
    DaemonStatus,
    /// Requests one complete reconciliation of every authoritative watched root.
    Reconcile,
    /// Activates one already-selected immutable certificate generation.
    ActivateGatewayCertificate { generation: String },
    /// Samples exact owned workload-plane resources without mutating them.
    BenchmarkSnapshot { require_converged: bool },
    /// Resolves exact mutable registry sources through the selected Engine.
    ResolveImageReferences {
        references: BTreeMap<String, String>,
    },
    /// Explicitly adopts retained state for one exact registered project path.
    AdoptProject { canonical_path: PathBuf },
    /// Reads secret-free durable status for one exact registered project path.
    ProjectStatus { canonical_path: PathBuf },
    /// Reads all retained project resources after their registry rows are gone.
    RetainedProjectStatus,
    /// Reads durable migration checkpoints for one exact registered project.
    ProjectMigrations { canonical_path: PathBuf },
    /// Queues one explicit decision for an exact reversible migration.
    DecideProjectMigration {
        canonical_path: PathBuf,
        migration_id: String,
        decision: IpcMigrationDecision,
    },
    /// Reads durable verified recovery points for one registered project.
    ProjectRecoveryPoints { canonical_path: PathBuf },
    /// Proves complete installation deletion intent without mutation.
    PlanInstallationDeletion,
    /// Freezes and starts exact installation deletion after token revalidation.
    ExecuteInstallationDeletion { confirmation_token: String },
    /// Reads terminal progress for an explicitly started installation deletion.
    InstallationDeletionStatus,
    /// Plans an exact retained PostgreSQL tenant deletion without mutation.
    PlanPostgresPrune {
        project_id: String,
        service_id: String,
        recovery_point_id: String,
    },
    /// Queues exact PostgreSQL deletion after confirmation-token revalidation.
    ExecutePostgresPrune {
        project_id: String,
        service_id: String,
        recovery_point_id: String,
        confirmation_token: String,
    },
    /// Explicitly exports daemon-owned values for one exact registered project.
    ProjectEnvironment { canonical_path: PathBuf },
    /// Opens one ownership-scoped, bounded in-memory container log session.
    OpenProjectLogs {
        canonical_path: PathBuf,
        services: Vec<String>,
        all: bool,
        follow: bool,
        tail: Option<u32>,
    },
    /// Reads an ordered page from one active project log session.
    PollProjectLogs {
        session_id: String,
        after_sequence: Option<u64>,
        max_chunks: u16,
    },
    /// Cancels an active request or stream by request ID.
    Cancel { target_request_id: String },
    /// Starts or resumes the ordered daemon event stream.
    SubscribeEvents { after_sequence: Option<u64> },
    /// Queues one bounded non-shell command in an owned project application.
    RunProjectCommand {
        canonical_path: PathBuf,
        service: String,
        command: IpcProjectCommand,
        timeout_seconds: u64,
    },
    /// Queues one verified logical recovery point for an owned project service.
    BackupProjectService {
        canonical_path: PathBuf,
        service: String,
    },
    /// Queues one reversible restore from an exact verified recovery point.
    RestoreProjectService {
        canonical_path: PathBuf,
        recovery_point_id: String,
    },
    /// Queues one explicit in-place SQL dump restore for an owned database.
    RestoreProjectDatabaseDump {
        canonical_path: PathBuf,
        service: String,
        file: PathBuf,
        archive_entry: Option<String>,
        reset: bool,
    },
}
