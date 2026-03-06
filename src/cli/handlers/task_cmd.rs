//! cli handlers task cmd module.
//!
//! Contains task command handling for opinionated workspace workflows.

use anyhow::{Context, Result};
use serde_json::Value;
use std::path::Path;

use crate::cli;
use crate::cli::args::PackageManagerArg;
use crate::config;
use crate::output::{self, LogLevel, Persistence::Persistent};

pub(crate) struct HandleTaskDepsBumpOptions<'a> {
    pub(crate) service: Option<&'a str>,
    pub(crate) kind: Option<config::Kind>,
    pub(crate) profile: Option<&'a str>,
    pub(crate) composer: bool,
    pub(crate) node: bool,
    pub(crate) all: bool,
    pub(crate) manager: Option<PackageManagerArg>,
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
            options.manager,
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
    requested_manager: Option<PackageManagerArg>,
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
    let manager = requested_manager.or_else(|| detect_node_manager(workspace_root));
    let manager = manager.context(
        "could not infer JS package manager from lockfiles; pass --manager <bun|npm|pnpm|yarn>",
    )?;

    super::log::info_if_not_quiet(
        quiet,
        "task",
        &format!(
            "Bumping {} dependencies for {package_name}",
            node_manager_name(manager)
        ),
    );

    for command in node_bump_commands(manager) {
        let display = command.join(" ");
        cli::support::run_service_command_with_tty(target, &command, tty, start_context)
            .with_context(|| format!("Node bump step failed for {package_name}: {display}"))?;
    }

    super::log::success_if_not_quiet(
        quiet,
        "task",
        &format!(
            "{} dependencies updated for {package_name}",
            node_manager_name(manager)
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

fn node_bump_commands(manager: PackageManagerArg) -> Vec<Vec<String>> {
    match manager {
        PackageManagerArg::Bun => vec![vec![
            "bun".to_owned(),
            "update".to_owned(),
            "--latest".to_owned(),
        ]],
        PackageManagerArg::Npm => vec![
            vec![
                "npx".to_owned(),
                "--yes".to_owned(),
                "npm-check-updates".to_owned(),
                "-u".to_owned(),
            ],
            vec!["npm".to_owned(), "install".to_owned()],
        ],
        PackageManagerArg::Pnpm => vec![vec![
            "pnpm".to_owned(),
            "update".to_owned(),
            "--latest".to_owned(),
        ]],
        PackageManagerArg::Yarn => vec![vec!["yarn".to_owned(), "up".to_owned(), "*".to_owned()]],
    }
}

fn node_manager_name(manager: PackageManagerArg) -> &'static str {
    match manager {
        PackageManagerArg::Bun => "bun",
        PackageManagerArg::Npm => "npm",
        PackageManagerArg::Pnpm => "pnpm",
        PackageManagerArg::Yarn => "yarn",
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

fn detect_node_manager(workspace_root: &Path) -> Option<PackageManagerArg> {
    [
        ("bun.lock", PackageManagerArg::Bun),
        ("bun.lockb", PackageManagerArg::Bun),
        ("pnpm-lock.yaml", PackageManagerArg::Pnpm),
        ("yarn.lock", PackageManagerArg::Yarn),
        ("package-lock.json", PackageManagerArg::Npm),
        ("npm-shrinkwrap.json", PackageManagerArg::Npm),
    ]
    .into_iter()
    .find_map(|(file_name, manager)| workspace_root.join(file_name).is_file().then_some(manager))
}

#[cfg(test)]
mod tests {
    use super::{
        composer_bump_commands, detect_node_manager, node_bump_commands, read_package_name,
        resolve_bump_targets,
    };
    use crate::cli::args::PackageManagerArg;

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
        let root = std::env::temp_dir().join(format!(
            "helm-task-node-manager-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("create temp root");
        std::fs::write(root.join("pnpm-lock.yaml"), "lockfileVersion: '9.0'").expect("lockfile");

        let manager = detect_node_manager(&root);
        assert!(matches!(manager, Some(PackageManagerArg::Pnpm)));

        std::fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn read_package_name_defaults_to_unknown_without_name() {
        let root = std::env::temp_dir().join(format!(
            "helm-task-read-package-name-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("create temp root");
        let manifest = root.join("composer.json");
        std::fs::write(&manifest, "{}").expect("write manifest");

        let package_name = read_package_name(&manifest).expect("package name");
        assert_eq!(package_name, "unknown");

        std::fs::remove_dir_all(&root).expect("cleanup");
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
            node_bump_commands(PackageManagerArg::Bun),
            vec![vec![
                "bun".to_owned(),
                "update".to_owned(),
                "--latest".to_owned(),
            ]]
        );
        assert_eq!(
            node_bump_commands(PackageManagerArg::Npm),
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
    }
}
