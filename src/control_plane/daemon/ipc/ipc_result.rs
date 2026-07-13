use super::{
    IpcEvent, IpcLogChunk, IpcLogSessionState, IpcManagedEnvironment, IpcMigrationStatus,
    IpcProjectStatus,
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
    /// Confirms an asynchronous operation was accepted.
    Accepted { operation_id: String },
    /// Reports one complete watched-root reconciliation attempt.
    Reconciled {
        project_count: usize,
        issue_count: usize,
        applied: bool,
    },
    /// Returns immutable manifest references keyed by the caller's exact IDs.
    ImageReferencesResolved {
        references: BTreeMap<String, String>,
    },
    /// Confirms exact retained state was atomically reactivated.
    ProjectAdopted { project_id: String },
    /// Returns one project status derived from authoritative daemon state.
    ProjectStatus { project: IpcProjectStatus },
    /// Returns stable migration checkpoints without credentials or recovery paths.
    ProjectMigrations { migrations: Vec<IpcMigrationStatus> },
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
