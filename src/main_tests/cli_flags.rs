use super::*;

#[test]
fn swarm_cli_defaults_to_dependency_expansion() {
    let cli = Cli::try_parse_from(["helm", "swarm", "up"]).expect("parse swarm command");
    match cli.command {
        Commands::Swarm(args) => assert!(!args.no_deps),
        _ => panic!("expected swarm command"),
    }

    let cli =
        Cli::try_parse_from(["helm", "swarm", "--no-deps", "up"]).expect("parse swarm command");
    match cli.command {
        Commands::Swarm(args) => assert!(args.no_deps),
        _ => panic!("expected swarm command"),
    }
}

#[test]
fn swarm_cli_parses_force_flag() {
    let cli =
        Cli::try_parse_from(["helm", "swarm", "--force", "down"]).expect("parse swarm command");
    match cli.command {
        Commands::Swarm(args) => assert!(args.force),
        _ => panic!("expected swarm command"),
    }
}

#[test]
fn up_and_down_cli_parse_project_dependency_flags() {
    let up_default = Cli::try_parse_from(["helm", "up"]).expect("parse up");
    match up_default.command {
        Commands::Up(args) => assert!(!args.no_deps),
        _ => panic!("expected up command"),
    }

    let up = Cli::try_parse_from(["helm", "up", "--no-deps"]).expect("parse up");
    match up.command {
        Commands::Up(args) => assert!(args.no_deps),
        _ => panic!("expected up command"),
    }

    let down = Cli::try_parse_from(["helm", "down", "--force"]).expect("parse down");
    match down.command {
        Commands::Down(args) => {
            assert!(!args.no_deps);
            assert!(args.force);
        }
        _ => panic!("expected down command"),
    }
}

#[test]
fn up_cli_parses_publish_all_flag() {
    let up = Cli::try_parse_from(["helm", "up", "--publish-all"]).expect("parse up");
    match up.command {
        Commands::Up(args) => assert!(args.publish_all),
        _ => panic!("expected up command"),
    }
}

#[test]
fn up_cli_parses_no_wait_and_no_publish_all_flags() {
    let up =
        Cli::try_parse_from(["helm", "up", "--no-wait", "--no-publish-all"]).expect("parse up");
    match up.command {
        Commands::Up(args) => {
            assert!(args.no_wait);
            assert!(args.no_publish_all);
        }
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
fn up_cli_parses_env_output_flag() {
    let up = Cli::try_parse_from(["helm", "up", "--env-output"]).expect("parse up");
    match up.command {
        Commands::Up(args) => assert!(args.env_output),
        _ => panic!("expected up command"),
    }
}

#[test]
fn up_cli_parses_seed_flag() {
    let up = Cli::try_parse_from(["helm", "up", "--seed"]).expect("parse up");
    match up.command {
        Commands::Up(args) => assert!(args.seed),
        _ => panic!("expected up command"),
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
fn swarm_cli_parses_env_output_flag() {
    let cli =
        Cli::try_parse_from(["helm", "swarm", "--env-output", "up"]).expect("parse swarm command");
    match cli.command {
        Commands::Swarm(args) => assert!(args.env_output),
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
fn cli_parses_ls_command() {
    let ls = Cli::try_parse_from(["helm", "ls"]).expect("parse ls");
    assert!(matches!(ls.command, Commands::Ls(_)));
}

#[test]
fn cli_parses_status_alias_as_ps_command() {
    let status = Cli::try_parse_from(["helm", "status"]).expect("parse status alias");
    assert!(matches!(status.command, Commands::Ps(_)));
}

#[test]
fn cli_parses_apply_and_update_commands() {
    let apply = Cli::try_parse_from(["helm", "apply"]).expect("parse apply");
    assert!(matches!(apply.command, Commands::Apply(_)));

    let update = Cli::try_parse_from(["helm", "update"]).expect("parse update");
    assert!(matches!(update.command, Commands::Update(_)));
}

#[test]
fn cli_rejects_removed_connect_list_shell_commands() {
    assert!(Cli::try_parse_from(["helm", "connect"]).is_err());
    assert!(Cli::try_parse_from(["helm", "list"]).is_err());
    assert!(Cli::try_parse_from(["helm", "shell"]).is_err());
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
fn doctor_cli_parses_reachability_flag() {
    let cli = Cli::try_parse_from(["helm", "doctor", "--reachability"])
        .expect("parse doctor reachability");
    match cli.command {
        Commands::Doctor(args) => assert!(args.reachability),
        _ => panic!("expected doctor command"),
    }
}

#[test]
fn start_cli_parses_core_flags() {
    let cli = Cli::try_parse_from([
        "helm",
        "start",
        "--profile",
        "app",
        "--no-open",
        "--no-wait",
        "--parallel",
        "4",
    ])
    .expect("parse start");
    match cli.command {
        Commands::Start(args) => {
            assert_eq!(args.profile.as_deref(), Some("app"));
            assert!(args.no_open);
            assert!(args.no_wait);
            assert_eq!(args.parallel, 4);
        }
        _ => panic!("expected start command"),
    }
}

#[test]
fn app_commands_parse_service_flag() {
    let serve = Cli::try_parse_from(["helm", "serve", "--service", "app"]).expect("parse serve");
    match serve.command {
        Commands::Serve(args) => assert_eq!(args.service.as_deref(), Some("app")),
        _ => panic!("expected serve command"),
    }

    let artisan = Cli::try_parse_from(["helm", "artisan", "--service", "app", "--", "about"])
        .expect("parse artisan");
    match artisan.command {
        Commands::Artisan(args) => assert_eq!(args.service.as_deref(), Some("app")),
        _ => panic!("expected artisan command"),
    }
}

#[test]
fn share_cli_parses_start_status_stop_subcommands() {
    let start = Cli::try_parse_from([
        "helm",
        "share",
        "start",
        "--service",
        "app",
        "--provider",
        "cloudflare",
        "--detached",
    ])
    .expect("parse share start");
    match start.command {
        Commands::Share(args) => match args.command {
            crate::cli::args::ShareCommands::Start(share_args) => {
                assert_eq!(share_args.service.as_deref(), Some("app"));
                assert!(matches!(
                    share_args.provider,
                    Some(crate::cli::args::ShareProviderArg::Cloudflare)
                ));
                assert!(share_args.detached);
            }
            _ => panic!("expected share start subcommand"),
        },
        _ => panic!("expected share command"),
    }

    let status = Cli::try_parse_from(["helm", "share", "status", "--provider", "tailscale"])
        .expect("parse share status");
    match status.command {
        Commands::Share(args) => match args.command {
            crate::cli::args::ShareCommands::Status(share_args) => assert!(matches!(
                share_args.provider,
                Some(crate::cli::args::ShareProviderArg::Tailscale)
            )),
            _ => panic!("expected share status subcommand"),
        },
        _ => panic!("expected share command"),
    }

    let shorthand = Cli::try_parse_from(["helm", "share", "start", "--tailscale"])
        .expect("parse share start shorthand");
    match shorthand.command {
        Commands::Share(args) => match args.command {
            crate::cli::args::ShareCommands::Start(share_args) => {
                assert!(share_args.provider.is_none());
                assert!(share_args.tailscale);
                assert!(!share_args.cloudflare);
            }
            _ => panic!("expected share start subcommand"),
        },
        _ => panic!("expected share command"),
    }

    let expose = Cli::try_parse_from(["helm", "share", "start", "--expose"])
        .expect("parse share start expose shorthand");
    match expose.command {
        Commands::Share(args) => match args.command {
            crate::cli::args::ShareCommands::Start(share_args) => {
                assert!(share_args.provider.is_none());
                assert!(share_args.expose);
                assert!(!share_args.cloudflare);
                assert!(!share_args.tailscale);
            }
            _ => panic!("expected share start subcommand"),
        },
        _ => panic!("expected share command"),
    }

    let timeout = Cli::try_parse_from(["helm", "share", "start", "--tailscale", "--timeout", "45"])
        .expect("parse share start timeout");
    match timeout.command {
        Commands::Share(args) => match args.command {
            crate::cli::args::ShareCommands::Start(share_args) => {
                assert_eq!(share_args.timeout, 45)
            }
            _ => panic!("expected share start subcommand"),
        },
        _ => panic!("expected share command"),
    }

    let stop = Cli::try_parse_from(["helm", "share", "stop", "--all"]).expect("parse share stop");
    match stop.command {
        Commands::Share(args) => match args.command {
            crate::cli::args::ShareCommands::Stop(share_args) => assert!(share_args.all),
            _ => panic!("expected share stop subcommand"),
        },
        _ => panic!("expected share command"),
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
