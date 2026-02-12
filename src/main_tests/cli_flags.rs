use super::*;

#[test]
fn swarm_cli_defaults_to_dependency_expansion() {
    let cli = Cli::try_parse_from(["helm", "swarm", "up"]).expect("parse swarm command");
    match cli.command {
        Commands::Swarm(args) => assert!(!args.without_deps),
        _ => panic!("expected swarm command"),
    }

    let cli = Cli::try_parse_from(["helm", "swarm", "--without-deps", "up"])
        .expect("parse swarm command");
    match cli.command {
        Commands::Swarm(args) => assert!(args.without_deps),
        _ => panic!("expected swarm command"),
    }
}

#[test]
fn swarm_cli_parses_force_down_deps_flag() {
    let cli = Cli::try_parse_from(["helm", "swarm", "--force-down-deps", "down"])
        .expect("parse swarm command");
    match cli.command {
        Commands::Swarm(args) => assert!(args.force_down_deps),
        _ => panic!("expected swarm command"),
    }
}

#[test]
fn up_and_down_cli_parse_project_dependency_flags() {
    let up_default = Cli::try_parse_from(["helm", "up"]).expect("parse up");
    match up_default.command {
        Commands::Up(args) => assert!(!args.with_project_deps),
        _ => panic!("expected up command"),
    }

    let up = Cli::try_parse_from(["helm", "up", "--with-project-deps"]).expect("parse up");
    match up.command {
        Commands::Up(args) => assert!(args.with_project_deps),
        _ => panic!("expected up command"),
    }

    let down = Cli::try_parse_from([
        "helm",
        "down",
        "--with-project-deps",
        "--force-project-dep-down",
    ])
    .expect("parse down");
    match down.command {
        Commands::Down(args) => {
            assert!(args.with_project_deps);
            assert!(args.force_project_dep_down);
        }
        _ => panic!("expected down command"),
    }
}

#[test]
fn up_cli_parses_force_random_ports_flag() {
    let up = Cli::try_parse_from(["helm", "up", "--force-random-ports"]).expect("parse up");
    match up.command {
        Commands::Up(args) => assert!(args.force_random_ports),
        _ => panic!("expected up command"),
    }
}

#[test]
fn up_cli_parses_port_strategy_and_seed_flags() {
    let up = Cli::try_parse_from([
        "helm",
        "up",
        "--port-strategy",
        "stable",
        "--port-seed",
        "workspace",
    ])
    .expect("parse up");
    match up.command {
        Commands::Up(args) => {
            assert!(matches!(args.port_strategy, PortStrategyArg::Stable));
            assert_eq!(args.port_seed.as_deref(), Some("workspace"));
        }
        _ => panic!("expected up command"),
    }
}

#[test]
fn up_cli_parses_write_env_flag() {
    let up = Cli::try_parse_from(["helm", "up", "--write-env"]).expect("parse up");
    match up.command {
        Commands::Up(args) => assert!(args.write_env),
        _ => panic!("expected up command"),
    }
}

#[test]
fn up_cli_parses_with_data_flag() {
    let up = Cli::try_parse_from(["helm", "up", "--with-data"]).expect("parse up");
    match up.command {
        Commands::Up(args) => assert!(args.with_data),
        _ => panic!("expected up command"),
    }
}

#[test]
fn swarm_cli_parses_force_random_ports_flag() {
    let cli = Cli::try_parse_from(["helm", "swarm", "--force-random-ports", "up"])
        .expect("parse swarm command");
    match cli.command {
        Commands::Swarm(args) => assert!(args.force_random_ports),
        _ => panic!("expected swarm command"),
    }
}

#[test]
fn swarm_cli_parses_port_strategy_and_seed_flags() {
    let cli = Cli::try_parse_from([
        "helm",
        "swarm",
        "--port-strategy",
        "stable",
        "--port-seed",
        "workspace",
        "up",
    ])
    .expect("parse swarm command");
    match cli.command {
        Commands::Swarm(args) => {
            assert!(matches!(args.port_strategy, PortStrategyArg::Stable));
            assert_eq!(args.port_seed.as_deref(), Some("workspace"));
        }
        _ => panic!("expected swarm command"),
    }
}

#[test]
fn swarm_cli_parses_write_env_flag() {
    let cli =
        Cli::try_parse_from(["helm", "swarm", "--write-env", "up"]).expect("parse swarm command");
    match cli.command {
        Commands::Swarm(args) => assert!(args.write_env),
        _ => panic!("expected swarm command"),
    }
}

#[test]
fn env_cli_persist_runtime_requires_sync_and_parses_when_enabled() {
    let invalid = Cli::try_parse_from(["helm", "env", "--persist-runtime"]);
    assert!(invalid.is_err());

    let valid = Cli::try_parse_from(["helm", "env", "--sync", "--persist-runtime"])
        .expect("parse env persist-runtime");
    match valid.command {
        Commands::Env(args) => {
            assert!(args.sync);
            assert!(!args.purge);
            assert!(args.persist_runtime);
            assert!(!args.create_missing);
        }
        _ => panic!("expected env command"),
    }
}

#[test]
fn env_cli_parses_generate_subcommand() {
    let cli = Cli::try_parse_from(["helm", "env", "generate", "--output", ".env.generated"])
        .expect("parse env generate");
    match cli.command {
        Commands::Env(args) => assert!(matches!(
            args.command,
            Some(crate::cli::args::EnvCommands::Generate { .. })
        )),
        _ => panic!("expected env command"),
    }
}

#[test]
fn cli_parses_global_runtime_env_flag() {
    let cli = Cli::try_parse_from(["helm", "--env", "test", "up"]).expect("parse global env");
    assert_eq!(cli.env.as_deref(), Some("test"));
}

#[test]
fn cli_parses_global_repro_flag() {
    let cli = Cli::try_parse_from(["helm", "--repro", "up"]).expect("parse global repro");
    assert!(cli.repro);
}

#[test]
fn cli_parses_about_command() {
    let cli = Cli::try_parse_from(["helm", "about"]).expect("parse about");
    assert!(matches!(cli.command, Commands::About(_)));
}

#[test]
fn cli_parses_apply_command() {
    let cli = Cli::try_parse_from(["helm", "apply"]).expect("parse apply");
    assert!(matches!(cli.command, Commands::Apply(_)));
}

#[test]
fn config_cli_parses_migrate_subcommand() {
    let cli = Cli::try_parse_from(["helm", "config", "migrate"]).expect("parse config migrate");
    match cli.command {
        Commands::Config(args) => assert!(matches!(
            args.command,
            Some(crate::cli::args::ConfigCommands::Migrate)
        )),
        _ => panic!("expected config command"),
    }
}

#[test]
fn doctor_cli_parses_repro_flag() {
    let cli = Cli::try_parse_from(["helm", "doctor", "--repro"]).expect("parse doctor repro");
    match cli.command {
        Commands::Doctor(args) => assert!(args.repro),
        _ => panic!("expected doctor command"),
    }
}

#[test]
fn lock_cli_parses_subcommands() {
    let images = Cli::try_parse_from(["helm", "lock", "images"]).expect("parse lock images");
    match images.command {
        Commands::Lock(args) => assert!(matches!(
            args.command,
            crate::cli::args::LockCommands::Images
        )),
        _ => panic!("expected lock command"),
    }

    let verify = Cli::try_parse_from(["helm", "lock", "verify"]).expect("parse lock verify");
    match verify.command {
        Commands::Lock(args) => assert!(matches!(
            args.command,
            crate::cli::args::LockCommands::Verify
        )),
        _ => panic!("expected lock command"),
    }

    let diff = Cli::try_parse_from(["helm", "lock", "diff"]).expect("parse lock diff");
    match diff.command {
        Commands::Lock(args) => {
            assert!(matches!(args.command, crate::cli::args::LockCommands::Diff))
        }
        _ => panic!("expected lock command"),
    }
}
