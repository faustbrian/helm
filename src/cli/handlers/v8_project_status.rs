//! Authoritative strict v8 project-status IPC client.

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Result, anyhow, bail};

use crate::control_plane::{
    IpcDiagnostic, IpcOutcome, IpcPayload, IpcProjectStatus, IpcRequest, IpcResult,
    default_unix_daemon_runtime_directory, send_unix_request,
};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
static STATUS_REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(super) fn request_v8_project_status(project_root: &Path) -> Result<IpcProjectStatus> {
    let request_id = format!(
        "project-status-{}-{}",
        std::process::id(),
        STATUS_REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let socket_path = default_unix_daemon_runtime_directory()?.join("daemon.sock");
    let response = send_unix_request(
        &socket_path,
        &IpcRequest::new(
            request_id,
            IpcPayload::ProjectStatus {
                canonical_path: project_root.to_path_buf(),
            },
        ),
        REQUEST_TIMEOUT,
    )?;

    match response.outcome() {
        IpcOutcome::Success {
            result: IpcResult::ProjectStatus { project },
        } => Ok(project.clone()),
        IpcOutcome::Success { .. } => bail!("daemon returned an unexpected status response"),
        IpcOutcome::Failure { diagnostics } => Err(diagnostic_error(diagnostics)),
    }
}

fn diagnostic_error(diagnostics: &[IpcDiagnostic]) -> anyhow::Error {
    let message = diagnostics
        .iter()
        .map(|diagnostic| format!("{}: {}", diagnostic.code(), diagnostic.message()))
        .collect::<Vec<_>>()
        .join("; ");
    anyhow!("daemon request failed: {message}")
}
