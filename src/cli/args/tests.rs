//! Strict v8 CLI surface tests.

use std::path::Path;

use clap::{CommandFactory, Parser};

use crate::cli::args::{Cli, Commands, ConfigCommands, EnvCommands, LockCommands};

#[test]
fn clean_slate_cli_rejects_removed_pre_v8_commands() {
    let command = Cli::command();
    let exposed = command
        .get_subcommands()
        .map(clap::Command::get_name)
        .collect::<Vec<_>>();

    for removed in [
        "init",
        "preset",
        "profile",
        "doctor",
        "start",
        "up",
        "apply",
        "update",
        "down",
        "stop",
        "rm",
        "recreate",
        "restart",
        "relabel",
        "restore",
        "dump",
        "about",
        "health",
        "top",
        "stats",
        "inspect",
        "attach",
        "cp",
        "kill",
        "pause",
        "unpause",
        "wait",
        "events",
        "port",
        "prune",
        "pull",
        "app-create",
        "ls",
        "swarm",
        "serve",
        "share",
        "env-scrub",
    ] {
        assert!(
            !exposed.contains(&removed),
            "removed command '{removed}' remains exposed"
        );
    }
}

#[test]
fn setup_accepts_one_or_more_watched_roots() {
    let cli = Cli::parse_from([
        "stackctl",
        "setup",
        "--dir",
        "/Users/example/Developer",
        "--dir",
        "/Users/example/Work",
        "--interval",
        "45",
    ]);

    let Commands::Setup(args) = cli.command else {
        panic!("setup command");
    };
    assert_eq!(
        args.dir,
        [
            std::path::PathBuf::from("/Users/example/Developer"),
            std::path::PathBuf::from("/Users/example/Work")
        ]
    );
    assert_eq!(args.interval, 45);
}

#[test]
fn daemon_retained_accepts_machine_readable_output() {
    let cli = Cli::parse_from(["stackctl", "daemon", "retained", "--format", "json"]);

    let Commands::Daemon(args) = cli.command else {
        panic!("daemon command");
    };
    let crate::cli::args::DaemonCommands::Retained(args) = args.command else {
        panic!("retained command");
    };
    assert_eq!(args.format, "json");
}

#[test]
fn clean_slate_cli_rejects_removed_pre_v8_global_flags() {
    let command = Cli::command();
    let exposed = command
        .get_arguments()
        .filter_map(clap::Arg::get_long)
        .collect::<Vec<_>>();

    for removed in [
        "env",
        "engine",
        "docker-max-heavy-ops",
        "docker-max-build-ops",
        "docker-retry-budget",
        "test-runtime-pool-size",
        "repro",
    ] {
        assert!(
            !exposed.contains(&removed),
            "removed global flag '--{removed}' remains exposed"
        );
    }
}

#[test]
fn global_v8_paths_and_behavior_flags_parse() {
    let cli = Cli::parse_from([
        "stackctl",
        "--quiet",
        "--no-color",
        "--dry-run",
        "--non-interactive",
        "--config",
        "/tmp/.stackctl.yaml",
        "status",
        "--format",
        "json",
    ]);

    assert!(cli.quiet);
    assert!(cli.no_color);
    assert!(cli.dry_run);
    assert!(cli.non_interactive);
    assert_eq!(cli.config_path(), Some(Path::new("/tmp/.stackctl.yaml")));
    assert!(matches!(cli.command, Commands::Ps(_)));
}

#[test]
fn benchmark_accepts_an_exact_expected_project_count() {
    let cli = Cli::try_parse_from(["stackctl", "daemon", "benchmark", "--expect-projects", "40"]);

    assert!(cli.is_ok());
}

#[test]
fn benchmark_accepts_a_typed_evidence_scenario() {
    let cli = Cli::try_parse_from([
        "stackctl",
        "daemon",
        "benchmark",
        "--evidence-scenario",
        "v8-forty-split",
    ]);

    assert!(cli.is_ok());
}

#[test]
fn strict_v8_configuration_and_lock_commands_are_explicit() {
    let schema = Cli::parse_from(["stackctl", "config", "schema"]);
    assert!(matches!(
        schema.command,
        Commands::Config(crate::cli::args::commands::ConfigArgs {
            command: ConfigCommands::Schema
        })
    ));

    let lock = Cli::parse_from(["stackctl", "lock", "verify"]);
    assert!(matches!(
        lock.command,
        Commands::Lock(crate::cli::args::commands::LockArgs {
            command: LockCommands::Verify
        })
    ));

    let env = Cli::parse_from([
        "stackctl",
        "env",
        "generate",
        "--output",
        "/tmp/stackctl.env",
    ]);
    assert!(matches!(
        env.command,
        Commands::Env(crate::cli::args::commands::EnvArgs {
            command: EnvCommands::Generate { .. }
        })
    ));
}

#[test]
fn project_commands_expose_only_v8_selectors() {
    let exec = Cli::parse_from([
        "stackctl",
        "exec",
        "--service",
        "worker",
        "php",
        "artisan",
        "queue:work",
    ]);
    let Commands::Exec(args) = exec.command else {
        panic!("exec command");
    };
    assert_eq!(args.service(), Some("worker"));
    assert_eq!(args.command, ["php", "artisan", "queue:work"]);

    let command = Cli::command();
    let exec = command
        .find_subcommand("exec")
        .expect("exec command definition");
    let exposed = exec
        .get_arguments()
        .filter_map(clap::Arg::get_long)
        .collect::<Vec<_>>();
    for removed in ["kind", "profile", "tty", "no-tty"] {
        assert!(
            !exposed.contains(&removed),
            "removed project selector '--{removed}' remains exposed"
        );
    }
}
