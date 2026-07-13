use super::IpcProjectCommand;
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
    /// Requests one complete reconciliation of every authoritative watched root.
    Reconcile,
    /// Resolves exact mutable registry sources through the selected Engine.
    ResolveImageReferences {
        references: BTreeMap<String, String>,
    },
    /// Explicitly adopts retained state for one exact registered project path.
    AdoptProject { canonical_path: PathBuf },
    /// Reads secret-free durable status for one exact registered project path.
    ProjectStatus { canonical_path: PathBuf },
    /// Reads durable migration checkpoints for one exact registered project.
    ProjectMigrations { canonical_path: PathBuf },
    /// Explicitly exports daemon-owned values for one exact registered project.
    ProjectEnvironment { canonical_path: PathBuf },
    /// Opens one ownership-scoped, bounded in-memory container log session.
    OpenProjectLogs {
        canonical_path: PathBuf,
        services: Vec<String>,
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
}
