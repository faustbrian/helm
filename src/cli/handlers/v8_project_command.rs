//! Strict v8 project command dispatch through singleton-daemon IPC.

use std::io::{Write, stderr, stdout};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
use base64::Engine as _;

use crate::cli::args::{Cli, Commands, PhpToolArgs};
use crate::cli::dispatch::context::CliDispatchContext;
use crate::control_plane::{
    IpcEventKind, IpcNodePackageManager, IpcOutcome, IpcOutputStream, IpcPayload, IpcPhpTool,
    IpcProjectCommand, IpcRequest, IpcResponse, IpcResult, default_unix_daemon_runtime_directory,
    send_unix_request,
};
use crate::javascript::{PackageManager, detect_node_package_manager};

use super::retry_daemon_request::retry_daemon_request;
use super::v8_project::resolve_v8_project;

const COMMAND_TIMEOUT_SECONDS: u64 = 3_600;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(50);
const STARTUP_RETRY_ATTEMPTS: usize = 600;
const STARTUP_RETRY_INTERVAL: Duration = Duration::from_secs(1);

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
            | Commands::Deno(_)
            | Commands::Phpstan(_)
            | Commands::Ecs(_)
            | Commands::PhpCsFixer(_)
            | Commands::Psalm(_)
            | Commands::Pint(_)
            | Commands::Pest(_)
            | Commands::Phpunit(_)
            | Commands::Rector(_)
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
            let browser = args.browser
                || args
                    .command
                    .iter()
                    .any(|argument| argument == "--browser" || argument.starts_with("--browser="));
            let arguments = args
                .command
                .iter()
                .filter(|argument| {
                    argument.as_str() != "--browser" && !argument.starts_with("--browser=")
                })
                .cloned()
                .collect();
            Ok((
                args.service().unwrap_or("app").to_owned(),
                IpcProjectCommand::Artisan { arguments, browser },
            ))
        }
        Commands::Exec(args) => {
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
        Commands::Bun(args) => Ok((
            args.service().unwrap_or("app").to_owned(),
            IpcProjectCommand::Bun {
                arguments: args.command.clone(),
            },
        )),
        Commands::Deno(args) => Ok((
            args.service().unwrap_or("app").to_owned(),
            IpcProjectCommand::Deno {
                arguments: args.command.clone(),
            },
        )),
        Commands::Phpstan(args) => php_tool_invocation(args, IpcPhpTool::PhpStan),
        Commands::Ecs(args) => php_tool_invocation(args, IpcPhpTool::Ecs),
        Commands::PhpCsFixer(args) => php_tool_invocation(args, IpcPhpTool::PhpCsFixer),
        Commands::Psalm(args) => php_tool_invocation(args, IpcPhpTool::Psalm),
        Commands::Pint(args) => php_tool_invocation(args, IpcPhpTool::Pint),
        Commands::Pest(args) => php_tool_invocation(args, IpcPhpTool::Pest),
        Commands::Phpunit(args) => php_tool_invocation(args, IpcPhpTool::PhpUnit),
        Commands::Rector(args) => php_tool_invocation(args, IpcPhpTool::Rector),
        _ => bail!("unsupported v8 project command"),
    }
}

fn php_tool_invocation(
    args: &PhpToolArgs,
    tool: IpcPhpTool,
) -> Result<(String, IpcProjectCommand)> {
    Ok((
        args.service().unwrap_or("app").to_owned(),
        IpcProjectCommand::PhpTool {
            tool,
            arguments: args.command.clone(),
        },
    ))
}

const fn ipc_package_manager(package_manager: PackageManager) -> IpcNodePackageManager {
    match package_manager {
        PackageManager::Npm => IpcNodePackageManager::Npm,
        PackageManager::Pnpm => IpcNodePackageManager::Pnpm,
        PackageManager::Yarn => IpcNodePackageManager::Yarn,
    }
}

fn execute_v8_invocation(invocation: V8ProjectInvocation) -> Result<()> {
    execute_project_command(
        invocation.project_root,
        invocation.service,
        invocation.command,
    )
}

pub(super) fn execute_project_command(
    project_root: PathBuf,
    service: String,
    command: IpcProjectCommand,
) -> Result<()> {
    let socket_path = default_unix_daemon_runtime_directory()?.join("daemon.sock");
    let operation_id = next_request_id("project-command");
    let response = retry_daemon_request(STARTUP_RETRY_ATTEMPTS, STARTUP_RETRY_INTERVAL, || {
        send_unix_request(
            &socket_path,
            &IpcRequest::new(
                operation_id.clone(),
                IpcPayload::RunProjectCommand {
                    canonical_path: project_root.clone(),
                    service: service.clone(),
                    command: command.clone(),
                    timeout_seconds: COMMAND_TIMEOUT_SECONDS,
                },
            ),
            REQUEST_TIMEOUT,
        )
        .map_err(Into::into)
    })?;
    let accepted_id = accepted_operation_id(&response)?;
    if accepted_id != operation_id {
        bail!("daemon accepted unexpected operation '{accepted_id}'");
    }

    follow_operation(&socket_path, &operation_id)
}

pub(super) fn follow_operation(socket_path: &Path, operation_id: &str) -> Result<()> {
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
                IpcEventKind::Diagnostics { .. } => {
                    bail!("project command returned an unexpected diagnostic snapshot")
                }
            }
        }
        std::thread::sleep(EVENT_POLL_INTERVAL);
    }
}

pub(super) fn accepted_operation_id(response: &IpcResponse) -> Result<&str> {
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

pub(super) fn next_request_id(kind: &str) -> String {
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
    use crate::control_plane::{IpcNodePackageManager, IpcPhpTool, IpcProjectCommand};

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
    fn unrelated_configuration_files_do_not_resolve_as_v8_invocations() {
        let root = project(
            "project.toml",
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
                browser: false,
            }
        );
    }

    #[test]
    fn v8_exec_requires_a_non_interactive_command() {
        assert!(Cli::try_parse_from(["stackctl", "exec",]).is_err());
    }

    #[test]
    fn v8_artisan_transports_browser_bootstrapping_without_shell_flags() {
        let root = project(
            ".stackctl.yaml",
            concat!(
                "schema_version: 8\nservices:\n  app:\n    preset: app\n",
                "  browser:\n    preset: dusk\n"
            ),
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

        let invocation = resolve_v8_invocation(&cli, &context)
            .expect("browser bootstrap")
            .expect("v8 invocation");

        assert_eq!(
            invocation.command,
            IpcProjectCommand::Artisan {
                arguments: vec!["test".to_owned()],
                browser: true,
            }
        );
    }

    #[test]
    fn v8_php_tools_use_a_whitelisted_container_executable() {
        let root = project(
            ".stackctl.yaml",
            "schema_version: 8\nservices:\n  app:\n    preset: app\n",
        );
        let cli = Cli::parse_from([
            "stackctl",
            "--project-root",
            root.to_str().expect("root"),
            "phpstan",
            "analyse",
            "--memory-limit=1G",
        ]);
        let context = CliDispatchContext::from_cli(&cli);

        let invocation = resolve_v8_invocation(&cli, &context)
            .expect("resolve invocation")
            .expect("v8 invocation");

        assert_eq!(
            invocation.command,
            IpcProjectCommand::PhpTool {
                tool: IpcPhpTool::PhpStan,
                arguments: vec!["analyse".to_owned(), "--memory-limit=1G".to_owned()],
            }
        );
    }
}
