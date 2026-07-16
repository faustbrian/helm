//! Strict v8 container logs through bounded singleton-daemon sessions.

use std::io::{Write, stderr, stdout};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine as _;

use crate::cli::args::{Cli, Commands, LogsArgs};
use crate::cli::dispatch::context::CliDispatchContext;
use crate::control_plane::{
    IpcDiagnostic, IpcLogChunk, IpcLogSessionState, IpcOutcome, IpcOutputStream, IpcPayload,
    IpcRequest, IpcResponse, IpcResult, default_unix_daemon_runtime_directory, send_unix_request,
};

use super::v8_project::resolve_v8_project;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(50);
const POLL_CHUNKS: u16 = 128;

static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Eq, PartialEq)]
struct V8LogInvocation {
    project_root: PathBuf,
    services: Vec<String>,
    all: bool,
    follow: bool,
    tail: Option<u32>,
    prefix: bool,
}

pub(crate) fn handle_v8_logs(cli: &Cli, context: &CliDispatchContext<'_>) -> Result<bool> {
    let Commands::Logs(args) = &cli.command else {
        return Ok(false);
    };
    let Some(project) = resolve_v8_project(context)? else {
        return Ok(false);
    };
    if context.dry_run() {
        bail!("--dry-run is not supported for v8 log sessions");
    }
    let invocation = log_invocation(args, project.root(), &project.service_names())?;
    execute_log_invocation(invocation)?;

    Ok(true)
}

fn log_invocation(
    args: &LogsArgs,
    project_root: &Path,
    declared_services: &[String],
) -> Result<V8LogInvocation> {
    let services = if args.all {
        declared_services.to_vec()
    } else if args.services().is_empty() {
        vec!["app".to_owned()]
    } else {
        args.services().to_vec()
    };
    for service in &services {
        if !declared_services.contains(service) {
            bail!("v8 service '{service}' is not declared in .stackctl.yaml");
        }
    }
    let tail = args
        .tail
        .map(|tail| u32::try_from(tail).context("v8 log tail exceeds the Engine API range"))
        .transpose()?;
    if tail == Some(0) {
        bail!("v8 log tail must be greater than zero");
    }

    Ok(V8LogInvocation {
        project_root: project_root.to_path_buf(),
        services,
        all: args.all,
        follow: args.follow,
        tail,
        prefix: args.prefix,
    })
}

fn execute_log_invocation(invocation: V8LogInvocation) -> Result<()> {
    let socket_path = default_unix_daemon_runtime_directory()?.join("daemon.sock");
    let session_id = next_request_id("project-logs");
    let response = send_unix_request(
        &socket_path,
        &IpcRequest::new(
            session_id.clone(),
            IpcPayload::OpenProjectLogs {
                canonical_path: invocation.project_root,
                services: invocation.services,
                all: invocation.all,
                follow: invocation.follow,
                tail: invocation.tail,
            },
        ),
        REQUEST_TIMEOUT,
    )?;
    let accepted = accepted_session_id(&response)?;
    if accepted != session_id {
        bail!("daemon accepted unexpected log session '{accepted}'");
    }
    let _session = LogSessionGuard::new(socket_path.clone(), session_id.clone());
    let mut cursor = None;

    loop {
        let response = send_unix_request(
            &socket_path,
            &IpcRequest::new(
                next_request_id("project-logs-poll"),
                IpcPayload::PollProjectLogs {
                    session_id: session_id.clone(),
                    after_sequence: cursor,
                    max_chunks: POLL_CHUNKS,
                },
            ),
            REQUEST_TIMEOUT,
        )?;
        let (chunks, latest_sequence, state) = log_page(&response, &session_id)?;
        for chunk in chunks {
            write_log_chunk(chunk, invocation.prefix)?;
        }
        cursor = Some(latest_sequence);
        match state {
            IpcLogSessionState::Starting | IpcLogSessionState::Streaming => {
                std::thread::sleep(POLL_INTERVAL);
            }
            IpcLogSessionState::Completed => return Ok(()),
            IpcLogSessionState::Failed { code, message } => {
                bail!("project logs failed ({code}): {message}")
            }
            IpcLogSessionState::Cancelled => bail!("project log session was cancelled"),
        }
    }
}

fn accepted_session_id(response: &IpcResponse) -> Result<&str> {
    match response.outcome() {
        IpcOutcome::Success {
            result: IpcResult::Accepted { operation_id },
        } => Ok(operation_id),
        IpcOutcome::Success { .. } => bail!("daemon returned an unexpected log response"),
        IpcOutcome::Failure { diagnostics } => Err(diagnostic_error(diagnostics)),
    }
}

fn log_page<'response>(
    response: &'response IpcResponse,
    expected_session_id: &str,
) -> Result<(&'response [IpcLogChunk], u64, IpcLogSessionState)> {
    match response.outcome() {
        IpcOutcome::Success {
            result:
                IpcResult::ProjectLogs {
                    session_id,
                    chunks,
                    latest_sequence,
                    state,
                },
        } if session_id == expected_session_id => Ok((chunks, *latest_sequence, state.clone())),
        IpcOutcome::Success { .. } => bail!("daemon returned an unexpected log page"),
        IpcOutcome::Failure { diagnostics } => Err(diagnostic_error(diagnostics)),
    }
}

fn write_log_chunk(chunk: &IpcLogChunk, prefix: bool) -> Result<()> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(chunk.data_base64())
        .context("daemon returned invalid Base64 log output")?;
    let writer: &mut dyn Write = match chunk.stream() {
        IpcOutputStream::Stdout => &mut stdout(),
        IpcOutputStream::Stderr => &mut stderr(),
    };
    if prefix {
        write!(writer, "{} | ", chunk.service())?;
    }
    writer.write_all(&bytes)?;
    writer.flush()?;

    Ok(())
}

fn diagnostic_error(diagnostics: &[IpcDiagnostic]) -> anyhow::Error {
    anyhow!(
        "daemon request failed: {}",
        diagnostics
            .iter()
            .map(|diagnostic| format!("{}: {}", diagnostic.code(), diagnostic.message()))
            .collect::<Vec<_>>()
            .join("; ")
    )
}

fn next_request_id(kind: &str) -> String {
    format!(
        "{kind}-{}-{}",
        std::process::id(),
        REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

struct LogSessionGuard {
    socket_path: PathBuf,
    session_id: String,
}

impl LogSessionGuard {
    fn new(socket_path: PathBuf, session_id: String) -> Self {
        Self {
            socket_path,
            session_id,
        }
    }
}

impl Drop for LogSessionGuard {
    fn drop(&mut self) {
        drop(send_unix_request(
            &self.socket_path,
            &IpcRequest::new(
                next_request_id("project-logs-cancel"),
                IpcPayload::Cancel {
                    target_request_id: self.session_id.clone(),
                },
            ),
            REQUEST_TIMEOUT,
        ));
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use clap::Parser;

    use crate::cli::args::{Cli, Commands};

    use super::log_invocation;

    #[test]
    fn logs_default_to_the_exact_app_service() {
        let cli = Cli::parse_from(["stackctl", "logs"]);
        let Commands::Logs(args) = cli.command else {
            panic!("logs command");
        };

        let invocation = log_invocation(
            &args,
            Path::new("/work/bill"),
            &["app".to_owned(), "db".to_owned()],
        )
        .expect("log invocation");

        assert_eq!(invocation.services, vec!["app"]);
        assert!(!invocation.all);
    }

    #[test]
    fn logs_all_selects_every_declared_service_without_invention() {
        let cli = Cli::parse_from(["stackctl", "logs", "--all"]);
        let Commands::Logs(args) = cli.command else {
            panic!("logs command");
        };

        let invocation = log_invocation(
            &args,
            Path::new("/work/bill"),
            &["app".to_owned(), "db".to_owned()],
        )
        .expect("log invocation");

        assert_eq!(invocation.services, vec!["app", "db"]);
        assert!(invocation.all);
    }

    #[test]
    fn logs_reject_host_access_logs_and_undeclared_services() {
        assert!(Cli::try_parse_from(["stackctl", "logs", "--access"]).is_err());

        let cli = Cli::parse_from(["stackctl", "logs", "--service", "missing"]);
        let Commands::Logs(args) = cli.command else {
            panic!("logs command");
        };
        let error = log_invocation(&args, Path::new("/work/bill"), &["app".to_owned()])
            .expect_err("undeclared service");
        assert!(error.to_string().contains("not declared"));
    }
}
