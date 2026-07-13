//! Strict v8 project command dispatch through singleton-daemon IPC.

use std::io::{Write, stderr, stdout};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine as _;

use crate::cli::args::{Cli, Commands};
use crate::cli::dispatch::context::CliDispatchContext;
use crate::control_plane::{
    IpcEventKind, IpcNodePackageManager, IpcOutcome, IpcOutputStream, IpcPayload,
    IpcProjectCommand, IpcRequest, IpcResponse, IpcResult, default_unix_daemon_runtime_directory,
    send_unix_request,
};
use crate::javascript::{PackageManager, detect_node_package_manager};

use super::v8_project::resolve_v8_project;

const COMMAND_TIMEOUT_SECONDS: u64 = 3_600;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(50);

static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Eq, PartialEq)]
struct V8ProjectInvocation {
    project_root: PathBuf,
    service: String,
    command: IpcProjectCommand,
}

pub(crate) fn handle_v8_project_command(
    cli: &Cli,
    context: &CliDispatchContext<'_>,
) -> Result<bool> {
    let Some(invocation) = resolve_v8_invocation(cli, context)? else {
        return Ok(false);
    };
    if context.dry_run() {
        bail!("--dry-run is not supported for v8 project commands");
    }

    execute_v8_invocation(invocation)?;
    Ok(true)
}

fn resolve_v8_invocation(
    cli: &Cli,
    context: &CliDispatchContext<'_>,
) -> Result<Option<V8ProjectInvocation>> {
    if !matches!(
        &cli.command,
        Commands::Artisan(_)
            | Commands::Exec(_)
            | Commands::Composer(_)
            | Commands::Node(_)
            | Commands::Bun(_)
    ) {
        return Ok(None);
    }

    let Some(project) = resolve_v8_project(context)? else {
        return Ok(None);
    };
    let (service, command) = command_from_cli(cli, project.root())?;
    if !project.has_service(&service) {
        bail!("v8 service '{service}' is not declared in .stackctl.yaml");
    }

    Ok(Some(V8ProjectInvocation {
        project_root: project.root().to_path_buf(),
        service,
        command,
    }))
}

fn command_from_cli(cli: &Cli, project_root: &Path) -> Result<(String, IpcProjectCommand)> {
    match &cli.command {
        Commands::Artisan(args) => {
            reject_legacy_selectors(args.kind.is_some(), args.profile())?;
            if args.browser || args.command.iter().any(|argument| argument == "--browser") {
                bail!(
                    "v8 artisan browser bootstrapping is not supported; run browser tests without --browser or use the v7 runtime"
                );
            }
            Ok((
                args.service().unwrap_or("app").to_owned(),
                IpcProjectCommand::Artisan {
                    arguments: args.command.clone(),
                },
            ))
        }
        Commands::Exec(args) => {
            reject_legacy_selectors(args.kind.is_some(), args.profile())?;
            if args.command.first().is_none_or(String::is_empty) {
                bail!(
                    "v8 exec requires a non-interactive command; interactive shells need daemon stdin and PTY support"
                );
            }
            Ok((
                args.service().unwrap_or("app").to_owned(),
                IpcProjectCommand::Exec {
                    arguments: args.command.clone(),
                },
            ))
        }
        Commands::Composer(args) => {
            reject_legacy_selectors(args.kind.is_some(), args.profile())?;
            let arguments = if args.command.is_empty() {
                vec!["list".to_owned()]
            } else {
                args.command.clone()
            };
            Ok((
                args.service().unwrap_or("app").to_owned(),
                IpcProjectCommand::Composer { arguments },
            ))
        }
        Commands::Node(args) => {
            reject_legacy_selectors(args.kind.is_some(), args.profile())?;
            if args.version_manager.is_some() || args.node_version.is_some() {
                bail!(
                    "v8 runtime versions are declarative; remove --version-manager and --node-version"
                );
            }
            let package_manager = args
                .package_manager
                .or_else(|| detect_node_package_manager(project_root))
                .context(
                    "could not infer a Node package manager; pass --package-manager <npm|pnpm|yarn>",
                )?;
            Ok((
                args.service().unwrap_or("app").to_owned(),
                IpcProjectCommand::NodePackageManager {
                    package_manager: ipc_package_manager(package_manager),
                    arguments: args.command.clone(),
                },
            ))
        }
        Commands::Bun(args) => {
            reject_legacy_selectors(args.kind.is_some(), args.profile())?;
            if args.bun_version.is_some() {
                bail!("v8 runtime versions are declarative; remove --bun-version");
            }
            Ok((
                args.service().unwrap_or("app").to_owned(),
                IpcProjectCommand::Bun {
                    arguments: args.command.clone(),
                },
            ))
        }
        _ => bail!("unsupported v8 project command"),
    }
}

fn reject_legacy_selectors(kind: bool, profile: Option<&str>) -> Result<()> {
    if kind || profile.is_some() {
        bail!("v8 commands require an exact --service; --kind and --profile are not supported");
    }
    Ok(())
}

const fn ipc_package_manager(package_manager: PackageManager) -> IpcNodePackageManager {
    match package_manager {
        PackageManager::Npm => IpcNodePackageManager::Npm,
        PackageManager::Pnpm => IpcNodePackageManager::Pnpm,
        PackageManager::Yarn => IpcNodePackageManager::Yarn,
    }
}

fn execute_v8_invocation(invocation: V8ProjectInvocation) -> Result<()> {
    let socket_path = default_unix_daemon_runtime_directory()?.join("daemon.sock");
    let operation_id = next_request_id("project-command");
    let response = send_unix_request(
        &socket_path,
        &IpcRequest::new(
            operation_id.clone(),
            IpcPayload::RunProjectCommand {
                canonical_path: invocation.project_root,
                service: invocation.service,
                command: invocation.command,
                timeout_seconds: COMMAND_TIMEOUT_SECONDS,
            },
        ),
        REQUEST_TIMEOUT,
    )?;
    let accepted_id = accepted_operation_id(&response)?;
    if accepted_id != operation_id {
        bail!("daemon accepted unexpected operation '{accepted_id}'");
    }

    follow_operation(&socket_path, &operation_id)
}

fn follow_operation(socket_path: &Path, operation_id: &str) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(COMMAND_TIMEOUT_SECONDS + 10);
    let mut cursor = None;
    loop {
        if Instant::now() >= deadline {
            bail!("timed out waiting for project command '{operation_id}'");
        }
        let response = send_unix_request(
            socket_path,
            &IpcRequest::new(
                next_request_id("events"),
                IpcPayload::SubscribeEvents {
                    after_sequence: cursor,
                },
            ),
            REQUEST_TIMEOUT,
        )?;
        let (events, latest_sequence) = event_result(&response)?;
        cursor = Some(latest_sequence);
        for event in events {
            if event.operation_id() != operation_id {
                continue;
            }
            match event.kind() {
                IpcEventKind::Accepted => {}
                IpcEventKind::Completed => return Ok(()),
                IpcEventKind::Failed { code, message } => {
                    bail!("project command failed ({code}): {message}")
                }
                IpcEventKind::Cancelled => bail!("project command was cancelled"),
                IpcEventKind::Output {
                    stream,
                    data_base64,
                } => write_output(*stream, data_base64)?,
            }
        }
        std::thread::sleep(EVENT_POLL_INTERVAL);
    }
}

fn accepted_operation_id(response: &IpcResponse) -> Result<&str> {
    match response.outcome() {
        IpcOutcome::Success {
            result: IpcResult::Accepted { operation_id },
        } => Ok(operation_id),
        IpcOutcome::Success { .. } => bail!("daemon returned an unexpected command response"),
        IpcOutcome::Failure { diagnostics } => Err(diagnostic_error(diagnostics)),
    }
}

fn event_result(response: &IpcResponse) -> Result<(&[crate::control_plane::IpcEvent], u64)> {
    match response.outcome() {
        IpcOutcome::Success {
            result:
                IpcResult::Events {
                    events,
                    latest_sequence,
                },
        } => Ok((events, *latest_sequence)),
        IpcOutcome::Success { .. } => bail!("daemon returned an unexpected event response"),
        IpcOutcome::Failure { diagnostics } => Err(diagnostic_error(diagnostics)),
    }
}

fn diagnostic_error(diagnostics: &[crate::control_plane::IpcDiagnostic]) -> anyhow::Error {
    let message = diagnostics
        .iter()
        .map(|diagnostic| format!("{}: {}", diagnostic.code(), diagnostic.message()))
        .collect::<Vec<_>>()
        .join("; ");
    anyhow!("daemon request failed: {message}")
}

fn write_output(stream: IpcOutputStream, encoded: &str) -> Result<()> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .context("daemon returned invalid Base64 command output")?;
    match stream {
        IpcOutputStream::Stdout => {
            stdout().write_all(&bytes)?;
            stdout().flush()?;
        }
        IpcOutputStream::Stderr => {
            stderr().write_all(&bytes)?;
            stderr().flush()?;
        }
    }
    Ok(())
}

fn next_request_id(kind: &str) -> String {
    format!(
        "{kind}-{}-{}",
        std::process::id(),
        REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use clap::Parser;

    use crate::cli::args::Cli;
    use crate::cli::dispatch::context::CliDispatchContext;
    use crate::control_plane::{IpcNodePackageManager, IpcProjectCommand};

    use super::resolve_v8_invocation;

    static PROJECT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    fn project(config_name: &str, contents: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "stackctl-v8-command-{}-{}",
            std::process::id(),
            PROJECT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create project");
        fs::write(root.join(config_name), contents).expect("write config");
        root
    }

    #[test]
    fn yaml_node_commands_resolve_exact_service_and_package_manager() {
        let root = project(
            ".stackctl.yaml",
            "schema_version: 8\nproject: bill\nservices:\n  app:\n    preset: app\n",
        );
        let cli = Cli::parse_from([
            "stackctl",
            "--project-root",
            root.to_str().expect("root"),
            "node",
            "--package-manager",
            "pnpm",
            "run",
            "build",
        ]);
        let context = CliDispatchContext::from_cli(&cli);

        let invocation = resolve_v8_invocation(&cli, &context)
            .expect("resolve invocation")
            .expect("v8 invocation");

        assert_eq!(invocation.service, "app");
        assert_eq!(invocation.project_root, root.canonicalize().expect("root"));
        assert_eq!(
            invocation.command,
            IpcProjectCommand::NodePackageManager {
                package_manager: IpcNodePackageManager::Pnpm,
                arguments: vec!["run".to_owned(), "build".to_owned()],
            }
        );
    }

    #[test]
    fn toml_projects_remain_on_the_legacy_dispatch_path() {
        let root = project(
            ".stackctl.toml",
            "schema_version = 1\nproject_type = \"project\"\nservice = []\nswarm = []\n",
        );
        let cli = Cli::parse_from([
            "stackctl",
            "--project-root",
            root.to_str().expect("root"),
            "composer",
            "install",
        ]);
        let context = CliDispatchContext::from_cli(&cli);

        assert!(
            resolve_v8_invocation(&cli, &context)
                .expect("resolve invocation")
                .is_none()
        );
    }

    #[test]
    fn v8_rejects_legacy_selector_autofixing() {
        let root = project(
            ".stackctl.yaml",
            "schema_version: 8\nservices:\n  app:\n    preset: app\n",
        );
        let cli = Cli::parse_from([
            "stackctl",
            "--project-root",
            root.to_str().expect("root"),
            "composer",
            "--kind",
            "app",
            "install",
        ]);
        let context = CliDispatchContext::from_cli(&cli);

        let error = resolve_v8_invocation(&cli, &context).expect_err("legacy selector");

        assert!(error.to_string().contains("--kind"));
        assert!(error.to_string().contains("--service"));
    }

    #[test]
    fn v8_requires_the_exact_selected_service() {
        let root = project(
            ".stackctl.yaml",
            "schema_version: 8\nservices:\n  web:\n    preset: app\n",
        );
        let cli = Cli::parse_from([
            "stackctl",
            "--project-root",
            root.to_str().expect("root"),
            "bun",
            "run",
            "build",
        ]);
        let context = CliDispatchContext::from_cli(&cli);

        let error = resolve_v8_invocation(&cli, &context).expect_err("missing app");

        assert!(error.to_string().contains("service 'app'"));
    }

    #[test]
    fn v8_artisan_commands_preserve_exact_arguments() {
        let root = project(
            ".stackctl.yaml",
            "schema_version: 8\nservices:\n  app:\n    preset: app\n",
        );
        let cli = Cli::parse_from([
            "stackctl",
            "--project-root",
            root.to_str().expect("root"),
            "artisan",
            "migrate",
            "--force",
        ]);
        let context = CliDispatchContext::from_cli(&cli);

        let invocation = resolve_v8_invocation(&cli, &context)
            .expect("resolve invocation")
            .expect("v8 invocation");

        assert_eq!(
            invocation.command,
            IpcProjectCommand::Artisan {
                arguments: vec!["migrate".to_owned(), "--force".to_owned()],
            }
        );
    }

    #[test]
    fn v8_exec_requires_a_non_interactive_command() {
        let root = project(
            ".stackctl.yaml",
            "schema_version: 8\nservices:\n  app:\n    preset: app\n",
        );
        let cli = Cli::parse_from([
            "stackctl",
            "--project-root",
            root.to_str().expect("root"),
            "exec",
        ]);
        let context = CliDispatchContext::from_cli(&cli);

        let error = resolve_v8_invocation(&cli, &context).expect_err("interactive exec");

        assert!(error.to_string().contains("non-interactive command"));
        assert!(error.to_string().contains("PTY"));
    }

    #[test]
    fn v8_artisan_rejects_unimplemented_browser_bootstrapping() {
        let root = project(
            ".stackctl.yaml",
            "schema_version: 8\nservices:\n  app:\n    preset: app\n",
        );
        let cli = Cli::parse_from([
            "stackctl",
            "--project-root",
            root.to_str().expect("root"),
            "artisan",
            "--browser",
            "test",
        ]);
        let context = CliDispatchContext::from_cli(&cli);

        let error = resolve_v8_invocation(&cli, &context).expect_err("browser bootstrap");

        assert!(error.to_string().contains("browser bootstrapping"));
    }
}
