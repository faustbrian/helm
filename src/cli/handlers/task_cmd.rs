//! cli handlers task cmd module.
//!
//! Contains task command handling for opinionated workspace workflows.

use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;

use crate::cli;
use crate::cli::args::{PackageManagerArg, VersionManagerArg};
use crate::config;
use crate::node::{
    BuildNodeCommandOptions, JavaScriptRuntime, ResolveNodeRuntimeOptions, build_node_command,
    resolve_node_runtime,
};
use crate::output::{self, LogLevel, Persistence::Persistent};

pub(crate) struct HandleTaskDepsBumpOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) kind: Option<config::Kind>,
    pub(crate) profile: Option<&'a str>,
    pub(crate) composer: bool,
    pub(crate) node: bool,
    pub(crate) all: bool,
    pub(crate) package_manager: Option<PackageManagerArg>,
    pub(crate) version_manager: Option<VersionManagerArg>,
    pub(crate) node_version: Option<&'a str>,
    pub(crate) non_interactive: bool,
    pub(crate) quiet: bool,
    pub(crate) tty: bool,
    pub(crate) no_tty: bool,
    pub(crate) config_path: Option<&'a Path>,
    pub(crate) project_root: Option<&'a Path>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TaskBumpTargets {
    composer: bool,
    node: bool,
}

pub(crate) fn handle_task_deps_bump(
    config: &config::Config,
    options: HandleTaskDepsBumpOptions<'_>,
) -> Result<()> {
    let targets = resolve_bump_targets(options.composer, options.node, options.all)?;
    let runtime = cli::support::resolve_app_runtime_context(
        config,
        resolve_single_app_service_name(config, options.service, options.kind, options.profile)?
            .as_deref(),
        options.config_path,
        options.project_root,
    )?;
    let tty = if options.non_interactive {
        false
    } else {
        cli::support::effective_tty(options.tty, options.no_tty)
    };
    let start_context = runtime.service_start_context();
    let workspace_root = runtime.workspace_root.as_path();

    if targets.composer {
        run_composer_bump(
            workspace_root,
            runtime.target,
            &start_context,
            tty,
            options.quiet,
        )?;
    }

    if targets.node {
        run_node_bump(
            workspace_root,
            runtime.target,
            &start_context,
            tty,
            options.quiet,
            options.package_manager,
            options.version_manager,
            options.node_version,
        )?;
    }

    Ok(())
}

fn resolve_single_app_service_name(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    profile: Option<&str>,
) -> Result<Option<String>> {
    if service.is_none() && kind.is_none() && profile.is_none() {
        return Ok(None);
    }

    let mut selected =
        cli::support::selected_services_with_filters(config, service, &[], kind, None, profile)?
            .into_iter()
            .filter(|svc| svc.kind == config::Kind::App)
            .collect::<Vec<_>>();

    if selected.is_empty() {
        anyhow::bail!("no app services matched the requested selector");
    }

    if selected.len() > 1 {
        anyhow::bail!("selector matched multiple app services; use --service to choose one");
    }

    Ok(selected
        .pop()
        .map(|service_config| service_config.name.clone()))
}

fn run_composer_bump(
    workspace_root: &Path,
    target: &config::ServiceConfig,
    start_context: &cli::support::ServiceStartContext<'_>,
    tty: bool,
    quiet: bool,
) -> Result<()> {
    let manifest_path = workspace_root.join("composer.json");
    if !manifest_path.is_file() {
        output::event(
            "task",
            LogLevel::Warn,
            &format!(
                "Skipping Composer bump: no composer.json in {}",
                workspace_root.display()
            ),
            Persistent,
        );
        return Ok(());
    }

    let package_name = read_package_name(&manifest_path).with_context(|| {
        format!(
            "failed to read package name from {}",
            manifest_path.display()
        )
    })?;

    super::log::info_if_not_quiet(
        quiet,
        "task",
        &format!("Bumping Composer dependencies for {package_name}"),
    );

    for command in composer_bump_commands() {
        let display = command.join(" ");
        cli::support::run_service_command_with_tty(target, &command, tty, start_context)
            .with_context(|| format!("Composer bump step failed for {package_name}: {display}"))?;
    }

    super::log::success_if_not_quiet(
        quiet,
        "task",
        &format!("Composer dependencies updated for {package_name}"),
    );

    Ok(())
}

fn run_node_bump(
    workspace_root: &Path,
    target: &config::ServiceConfig,
    start_context: &cli::support::ServiceStartContext<'_>,
    tty: bool,
    quiet: bool,
    requested_package_manager: Option<PackageManagerArg>,
    requested_version_manager: Option<VersionManagerArg>,
    requested_node_version: Option<&str>,
) -> Result<()> {
    let manifest_path = workspace_root.join("package.json");
    if !manifest_path.is_file() {
        output::event(
            "task",
            LogLevel::Warn,
            &format!(
                "Skipping Node bump: no package.json in {}",
                workspace_root.display()
            ),
            Persistent,
        );
        return Ok(());
    }

    let package_name = read_package_name(&manifest_path).with_context(|| {
        format!(
            "failed to read package name from {}",
            manifest_path.display()
        )
    })?;
    let node_runtime = resolve_node_runtime(ResolveNodeRuntimeOptions {
        configured: target.node.as_ref(),
        workspace_root,
        runtime: None,
        package_manager: requested_package_manager,
        version_manager: requested_version_manager,
        node_version: requested_node_version,
        require_package_manager: true,
    })?;
    let bump_workflow = resolve_bump_workflow(node_runtime.runtime, node_runtime.package_manager)?;

    super::log::info_if_not_quiet(
        quiet,
        "task",
        &format!(
            "Bumping {} dependencies for {package_name}",
            bump_workflow.display_name()
        ),
    );

    for command in bump_workflow.commands() {
        let wrapped = if bump_workflow.wrap_with_node_manager() {
            build_node_command(BuildNodeCommandOptions {
                version_manager: node_runtime.version_manager,
                node_version: node_runtime.node_version.as_deref(),
                command: &command,
            })?
        } else {
            command
        };
        let display = wrapped.join(" ");
        cli::support::run_service_command_with_tty(target, &wrapped, tty, start_context)
            .with_context(|| format!("Node bump step failed for {package_name}: {display}"))?;
    }

    super::log::success_if_not_quiet(
        quiet,
        "task",
        &format!(
            "{} dependencies updated for {package_name}",
            bump_workflow.display_name()
        ),
    );

    Ok(())
}

fn read_package_name(path: &Path) -> Result<String> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read manifest {}", path.display()))?;
    let parsed: Value = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse manifest {}", path.display()))?;

    Ok(parsed
        .get("name")
        .and_then(Value::as_str)
        .map_or_else(|| "unknown".to_owned(), str::to_owned))
}

fn composer_bump_commands() -> Vec<Vec<String>> {
    vec![
        vec![
            "composer".to_owned(),
            "bump".to_owned(),
            "--dev-only".to_owned(),
        ],
        vec![
            "composer".to_owned(),
            "bump".to_owned(),
            "--no-dev-only".to_owned(),
        ],
        vec![
            "composer".to_owned(),
            "update".to_owned(),
            "--ignore-platform-reqs".to_owned(),
        ],
        vec!["composer".to_owned(), "normalize".to_owned()],
    ]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DependencyBumpWorkflow {
    Bun,
    Npm,
    Pnpm,
    Yarn,
}

impl DependencyBumpWorkflow {
    fn commands(self) -> Vec<Vec<String>> {
        match self {
            Self::Bun => vec![vec![
                "bun".to_owned(),
                "update".to_owned(),
                "--latest".to_owned(),
            ]],
            Self::Npm => vec![
                vec![
                    "npx".to_owned(),
                    "--yes".to_owned(),
                    "npm-check-updates".to_owned(),
                    "-u".to_owned(),
                ],
                vec!["npm".to_owned(), "install".to_owned()],
            ],
            Self::Pnpm => vec![vec![
                "pnpm".to_owned(),
                "update".to_owned(),
                "--latest".to_owned(),
            ]],
            Self::Yarn => vec![vec!["yarn".to_owned(), "up".to_owned(), "*".to_owned()]],
        }
    }

    const fn display_name(self) -> &'static str {
        match self {
            Self::Bun => "bun",
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
        }
    }

    const fn wrap_with_node_manager(self) -> bool {
        !matches!(self, Self::Bun)
    }
}

fn resolve_bump_workflow(
    runtime: JavaScriptRuntime,
    package_manager: Option<PackageManagerArg>,
) -> Result<DependencyBumpWorkflow> {
    match runtime {
        JavaScriptRuntime::Bun => Ok(DependencyBumpWorkflow::Bun),
        JavaScriptRuntime::Node => package_manager
            .map(|manager| match manager {
                PackageManagerArg::Npm => DependencyBumpWorkflow::Npm,
                PackageManagerArg::Pnpm => DependencyBumpWorkflow::Pnpm,
                PackageManagerArg::Yarn => DependencyBumpWorkflow::Yarn,
            })
            .context("node package manager required"),
        JavaScriptRuntime::Deno => {
            anyhow::bail!(
                "configured JS runtime is deno; use `helm deno` instead of Node dependency bump workflows"
            );
        }
    }
}

fn resolve_bump_targets(composer: bool, node: bool, all: bool) -> Result<TaskBumpTargets> {
    if all {
        return Ok(TaskBumpTargets {
            composer: true,
            node: true,
        });
    }

    if composer || node {
        return Ok(TaskBumpTargets { composer, node });
    }

    anyhow::bail!("select at least one target with --composer, --node, or --all")
}

#[cfg(test)]
mod tests {
    use super::{
        DependencyBumpWorkflow, composer_bump_commands, read_package_name, resolve_bump_targets,
        resolve_bump_workflow,
    };
    use crate::cli::args::PackageManagerArg;
    use crate::node::{JavaScriptRuntime, detect_node_package_manager};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(prefix: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn resolve_bump_targets_accepts_explicit_flags() {
        let targets = resolve_bump_targets(true, false, false).expect("composer target");
        assert!(targets.composer);
        assert!(!targets.node);

        let targets = resolve_bump_targets(false, true, false).expect("node target");
        assert!(!targets.composer);
        assert!(targets.node);

        let targets = resolve_bump_targets(false, false, true).expect("all targets");
        assert!(targets.composer);
        assert!(targets.node);
    }

    #[test]
    fn resolve_bump_targets_requires_a_target_flag() {
        let error = resolve_bump_targets(false, false, false).expect_err("missing target");
        assert!(error.to_string().contains("select at least one target"));
    }

    #[test]
    fn detect_node_manager_prefers_matching_lockfile() {
        let root = temp_root("helm-task-node-manager");
        fs::write(root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'").expect("lockfile");

        let manager = detect_node_package_manager(&root);
        assert!(matches!(manager, Some(PackageManagerArg::Pnpm)));

        fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn detect_node_manager_reads_package_manager_from_package_json() {
        let root = temp_root("helm-task-node-package-manager");
        fs::write(
            root.join("package.json"),
            r#"{"name":"demo","packageManager":"yarn@4.6.0"}"#,
        )
        .expect("package json");

        let manager = detect_node_package_manager(&root);
        assert!(matches!(manager, Some(PackageManagerArg::Yarn)));

        fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn read_package_name_defaults_to_unknown_without_name() {
        let root = temp_root("helm-task-read-package-name");
        let manifest = root.join("composer.json");
        fs::write(&manifest, "{}").expect("write manifest");

        let package_name = read_package_name(&manifest).expect("package name");
        assert_eq!(package_name, "unknown");

        fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn composer_bump_commands_match_expected_workflow() {
        assert_eq!(
            composer_bump_commands(),
            vec![
                vec![
                    "composer".to_owned(),
                    "bump".to_owned(),
                    "--dev-only".to_owned(),
                ],
                vec![
                    "composer".to_owned(),
                    "bump".to_owned(),
                    "--no-dev-only".to_owned(),
                ],
                vec![
                    "composer".to_owned(),
                    "update".to_owned(),
                    "--ignore-platform-reqs".to_owned(),
                ],
                vec!["composer".to_owned(), "normalize".to_owned()],
            ]
        );
    }

    #[test]
    fn node_bump_commands_match_supported_managers() {
        assert_eq!(
            DependencyBumpWorkflow::Npm.commands(),
            vec![
                vec![
                    "npx".to_owned(),
                    "--yes".to_owned(),
                    "npm-check-updates".to_owned(),
                    "-u".to_owned(),
                ],
                vec!["npm".to_owned(), "install".to_owned()],
            ]
        );
        assert_eq!(
            DependencyBumpWorkflow::Pnpm.commands(),
            vec![vec![
                "pnpm".to_owned(),
                "update".to_owned(),
                "--latest".to_owned(),
            ]]
        );
        assert_eq!(
            DependencyBumpWorkflow::Yarn.commands(),
            vec![vec!["yarn".to_owned(), "up".to_owned(), "*".to_owned()]]
        );
    }

    #[test]
    fn resolve_bump_workflow_uses_bun_runtime_directly() {
        let workflow = resolve_bump_workflow(JavaScriptRuntime::Bun, None).expect("bun workflow");
        assert_eq!(workflow, DependencyBumpWorkflow::Bun);
        assert!(!workflow.wrap_with_node_manager());
    }

    #[test]
    fn resolve_bump_workflow_rejects_deno_runtime() {
        let error = resolve_bump_workflow(JavaScriptRuntime::Deno, None).expect_err("deno fails");
        assert!(error.to_string().contains("use `helm deno`"));
    }
}
