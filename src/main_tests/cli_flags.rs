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
fn cli_parses_global_docker_policy_flags() {
    let cli = Cli::try_parse_from([
        "helm",
        "--docker-max-heavy-ops",
        "3",
        "--docker-max-build-ops",
        "1",
        "--docker-retry-budget",
        "5",
        "--test-runtime-pool-size",
        "4",
        "up",
    ])
    .expect("parse docker policy flags");

    assert_eq!(cli.docker_max_heavy_ops, Some(3));
    assert_eq!(cli.docker_max_build_ops, Some(1));
    assert_eq!(cli.docker_retry_budget, Some(5));
    assert_eq!(cli.test_runtime_pool_size, Some(4));
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
fn doctor_cli_parses_json_format() {
    let cli = Cli::try_parse_from(["helm", "doctor", "--format", "json"])
        .expect("parse doctor json format");
    match cli.command {
        Commands::Doctor(args) => assert_eq!(args.format, "json"),
        _ => panic!("expected doctor command"),
    }
}

#[test]
fn health_cli_parses_json_format() {
    let cli = Cli::try_parse_from(["helm", "health", "--format", "json"])
        .expect("parse health json format");
    match cli.command {
        Commands::Health(args) => assert_eq!(args.format, "json"),
        _ => panic!("expected health command"),
    }
}

#[test]
fn about_cli_parses_json_format() {
    let cli = Cli::try_parse_from(["helm", "about", "--format", "json"])
        .expect("parse about json format");
    match cli.command {
        Commands::About(args) => assert_eq!(args.format, "json"),
        _ => panic!("expected about command"),
    }
}

#[test]
fn lifecycle_commands_parse_profile_flag() {
    let down = Cli::try_parse_from(["helm", "down", "--profile", "infra"]).expect("parse down");
    match down.command {
        Commands::Down(args) => assert_eq!(args.profile(), Some("infra")),
        _ => panic!("expected down command"),
    }

    let stop = Cli::try_parse_from(["helm", "stop", "--profile", "data"]).expect("parse stop");
    match stop.command {
        Commands::Stop(args) => assert_eq!(args.profile(), Some("data")),
        _ => panic!("expected stop command"),
    }

    let rm = Cli::try_parse_from(["helm", "rm", "--profile", "app"]).expect("parse rm");
    match rm.command {
        Commands::Rm(args) => assert_eq!(args.profile(), Some("app")),
        _ => panic!("expected rm command"),
    }

    let recreate =
        Cli::try_parse_from(["helm", "recreate", "--profile", "full"]).expect("parse recreate");
    match recreate.command {
        Commands::Recreate(args) => assert_eq!(args.profile(), Some("full")),
        _ => panic!("expected recreate command"),
    }

    let restart =
        Cli::try_parse_from(["helm", "restart", "--profile", "web"]).expect("parse restart");
    match restart.command {
        Commands::Restart(args) => assert_eq!(args.profile(), Some("web")),
        _ => panic!("expected restart command"),
    }
}

#[test]
fn down_stop_restart_parse_repeated_service_flags() {
    let down = Cli::try_parse_from(["helm", "down", "--service", "db", "--service", "cache"])
        .expect("parse down repeated service");
    match down.command {
        Commands::Down(args) => assert_eq!(args.services(), ["db", "cache"]),
        _ => panic!("expected down command"),
    }

    let stop = Cli::try_parse_from(["helm", "stop", "--service", "db", "--service", "cache"])
        .expect("parse stop repeated service");
    match stop.command {
        Commands::Stop(args) => assert_eq!(args.services(), ["db", "cache"]),
        _ => panic!("expected stop command"),
    }

    let restart = Cli::try_parse_from(["helm", "restart", "--service", "db", "--service", "cache"])
        .expect("parse restart repeated service");
    match restart.command {
        Commands::Restart(args) => assert_eq!(args.services(), ["db", "cache"]),
        _ => panic!("expected restart command"),
    }
}

#[test]
fn logs_cli_parses_since_until() {
    let cli = Cli::try_parse_from([
        "helm",
        "logs",
        "--since",
        "5m",
        "--until",
        "2026-02-20T10:00:00Z",
    ])
    .expect("parse logs since/until");
    match cli.command {
        Commands::Logs(args) => {
            assert_eq!(args.since.as_deref(), Some("5m"));
            assert_eq!(args.until.as_deref(), Some("2026-02-20T10:00:00Z"));
        }
        _ => panic!("expected logs command"),
    }
}

#[test]
fn cli_parses_non_interactive_global_flag() {
    let cli =
        Cli::try_parse_from(["helm", "--non-interactive", "up"]).expect("parse non interactive");
    assert!(cli.non_interactive);
}

#[test]
fn app_runtime_commands_parse_kind_and_profile_selectors() {
    let exec = Cli::try_parse_from(["helm", "exec", "--kind", "app", "--", "php", "-v"])
        .expect("parse exec kind selector");
    match exec.command {
        Commands::Exec(args) => {
            assert_eq!(args.kind, Some(crate::config::Kind::App));
            assert_eq!(args.profile(), None);
        }
        _ => panic!("expected exec command"),
    }

    let serve = Cli::try_parse_from(["helm", "serve", "--profile", "web"])
        .expect("parse serve profile selector");
    match serve.command {
        Commands::Serve(args) => assert_eq!(args.profile(), Some("web")),
        _ => panic!("expected serve command"),
    }

    let open = Cli::try_parse_from(["helm", "open", "--profile", "app", "--no-browser"])
        .expect("parse open profile selector");
    match open.command {
        Commands::Open(args) => assert_eq!(args.profile(), Some("app")),
        _ => panic!("expected open command"),
    }
}

#[test]
fn ops_commands_parse_profile_and_repeated_service_selectors() {
    let pull = Cli::try_parse_from(["helm", "pull", "--profile", "infra"]).expect("parse pull");
    match pull.command {
        Commands::Pull(args) => assert_eq!(args.profile(), Some("infra")),
        _ => panic!("expected pull command"),
    }

    let relabel = Cli::try_parse_from(["helm", "relabel", "--service", "db", "--service", "cache"])
        .expect("parse relabel repeated service");
    match relabel.command {
        Commands::Relabel(args) => assert_eq!(args.services(), ["db", "cache"]),
        _ => panic!("expected relabel command"),
    }

    let health =
        Cli::try_parse_from(["helm", "health", "--profile", "data"]).expect("parse health");
    match health.command {
        Commands::Health(args) => assert_eq!(args.profile(), Some("data")),
        _ => panic!("expected health command"),
    }

    let logs =
        Cli::try_parse_from(["helm", "logs", "--profile", "app"]).expect("parse logs profile");
    match logs.command {
        Commands::Logs(args) => assert_eq!(args.profile(), Some("app")),
        _ => panic!("expected logs command"),
    }
}

#[test]
fn docker_ops_parse_profile_and_repeated_services() {
    let inspect = Cli::try_parse_from([
        "helm",
        "inspect",
        "--service",
        "db",
        "--service",
        "cache",
        "--profile",
        "infra",
    ]);
    assert!(inspect.is_err());

    let inspect = Cli::try_parse_from(["helm", "inspect", "--service", "db", "--service", "cache"])
        .expect("parse inspect repeated service");
    match inspect.command {
        Commands::Inspect(args) => assert_eq!(args.services(), ["db", "cache"]),
        _ => panic!("expected inspect command"),
    }

    let port = Cli::try_parse_from(["helm", "port", "--format", "json"]).expect("parse port");
    match port.command {
        Commands::Port(args) => assert_eq!(args.format, "json"),
        _ => panic!("expected port command"),
    }

    let kill = Cli::try_parse_from(["helm", "kill", "--profile", "data"]).expect("parse kill");
    match kill.command {
        Commands::Kill(args) => assert_eq!(args.profile(), Some("data")),
        _ => panic!("expected kill command"),
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
fn app_artisan_cli_parses_browser_flag() {
    let artisan = Cli::try_parse_from(["helm", "artisan", "--browser", "test"])
        .expect("parse artisan");
    match artisan.command {
        Commands::Artisan(args) => {
            assert!(args.browser);
            assert_eq!(args.command, vec!["test".to_owned()]);
        }
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
