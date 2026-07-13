//! cli args tests module.
//!
//! Covers CLI argument methods and command parse entrypoints.

use std::path::{Path, PathBuf};

use clap::Parser;

use crate::cli::args::commands;
use crate::cli::args::{
    Cli, ConfigCommands, EnvCommands, LockCommands, PackageManagerArg, PresetCommands,
    ProfileCommands, ShareProviderArg, VersionManagerArg, default_parallelism,
};
use crate::config;

#[test]
fn default_parallelism_is_bounded() {
    assert!((1..=4).contains(&default_parallelism()));
}

#[test]
fn cli_accessors_reflect_global_flags() {
    let cli = Cli::try_parse_from([
        "stackctl",
        "--quiet",
        "--repro",
        "--config",
        "/tmp/config.toml",
        "--env",
        "test",
        "start",
        "--service",
        "api",
        "--wait",
        "--parallel",
        "2",
        "--no-open",
    ])
    .expect("parse cli");

    assert!(cli.quiet);
    assert!(cli.repro);
    assert_eq!(cli.config_path(), Some(Path::new("/tmp/config.toml")));
    assert_eq!(cli.runtime_env(), Some("test"));

    if let commands::Commands::Start(args) = cli.command {
        assert_eq!(args.service(), Some("api"));
        assert!(args.wait);
        assert_eq!(args.parallel, 2);
    } else {
        panic!("expected start command");
    }
}

#[test]
fn app_action_argument_methods() {
    let args = commands::AppCreateArgs {
        service: Some("api".to_owned()),
        no_migrate: true,
        seed: false,
        no_storage_link: true,
    };
    assert_eq!(args.service(), Some("api"));

    let args = commands::ServeArgs {
        service: Some("api".to_owned()),
        kind: None,
        profile: None,
        recreate: true,
        detached: true,
        env_output: false,
        trust_container_ca: true,
    };
    assert_eq!(args.service(), Some("api"));

    let args = commands::OpenArgs {
        service: None,
        kind: None,
        profile: None,
        all: true,
        health_path: Some("/ready".to_owned()),
        no_browser: false,
        database: true,
        json: true,
    };
    assert_eq!(args.health_path(), Some("/ready"));
    assert!(args.all);
    assert!(args.database);

    let args = commands::EnvScrubArgs {
        env_file: Some(PathBuf::from("/tmp/.env")),
    };
    assert_eq!(args.env_file(), Some(Path::new("/tmp/.env")));

    let args = commands::ExecArgs {
        service: Some("app".to_owned()),
        kind: None,
        profile: None,
        tty: true,
        no_tty: false,
        command: vec!["sh".to_owned()],
    };
    assert_eq!(args.service(), Some("app"));

    let args = commands::ArtisanArgs {
        service: None,
        kind: None,
        profile: None,
        browser: false,
        tty: false,
        no_tty: true,
        command: vec!["migrate".to_owned()],
    };
    assert!(args.service().is_none());

    let args = commands::ComposerArgs {
        service: Some("worker".to_owned()),
        kind: None,
        profile: None,
        tty: true,
        no_tty: false,
        command: vec!["install".to_owned()],
    };
    assert_eq!(args.service(), Some("worker"));

    let args = commands::PhpToolArgs {
        service: Some("app".to_owned()),
        kind: None,
        profile: Some("web".to_owned()),
        tty: false,
        no_tty: true,
        command: vec!["analyse".to_owned()],
    };
    assert_eq!(args.profile(), Some("web"));

    let args = commands::NodeArgs {
        service: Some("node".to_owned()),
        kind: None,
        profile: None,
        package_manager: Some(PackageManagerArg::Pnpm),
        version_manager: Some(VersionManagerArg::Fnm),
        node_version: Some("22".to_owned()),
        tty: true,
        no_tty: false,
        command: vec!["run".to_owned(), "dev".to_owned()],
    };
    assert_eq!(args.service(), Some("node"));

    let args = commands::BunArgs {
        service: Some("bun".to_owned()),
        kind: None,
        profile: None,
        bun_version: Some("1.2.5".to_owned()),
        tty: true,
        no_tty: false,
        command: vec!["run".to_owned(), "dev".to_owned()],
    };
    assert_eq!(args.service(), Some("bun"));

    let args = commands::DenoArgs {
        service: Some("app".to_owned()),
        kind: None,
        profile: None,
        deno_version: Some("2.2.3".to_owned()),
        tty: true,
        no_tty: false,
        command: vec!["task".to_owned(), "dev".to_owned()],
    };
    assert_eq!(args.service(), Some("app"));
}

#[test]
fn app_share_argument_methods() {
    let start = commands::ShareStartArgs {
        service: Some("api".to_owned()),
        provider: Some(ShareProviderArg::Cloudflare),
        cloudflare: false,
        expose: false,
        tailscale: false,
        detached: false,
        timeout: 15,
        json: true,
    };
    assert_eq!(start.service(), Some("api"));
    assert_eq!(
        start.provider_selection().provider,
        Some(ShareProviderArg::Cloudflare)
    );

    let status = commands::ShareStatusArgs {
        service: Some("api".to_owned()),
        provider: Some(ShareProviderArg::Expose),
        cloudflare: false,
        expose: true,
        tailscale: false,
        json: false,
    };
    assert_eq!(
        status.provider_selection().provider,
        Some(ShareProviderArg::Expose)
    );
    assert!(status.provider_selection().expose);

    let stop = commands::ShareStopArgs {
        service: None,
        provider: Some(ShareProviderArg::Tailscale),
        cloudflare: true,
        expose: false,
        tailscale: false,
        all: false,
        json: true,
    };
    assert_eq!(
        stop.provider_selection().provider,
        Some(ShareProviderArg::Tailscale)
    );
    assert!(stop.provider_selection().cloudflare);
    assert!(!stop.provider_selection().tailscale);
}

#[test]
fn lifecycle_argument_methods() {
    let cli = Cli::parse_from([
        "stackctl",
        "url",
        "--service",
        "db",
        "--format",
        "json",
        "--kind",
        "database",
        "--driver",
        "mysql",
    ]);
    if let commands::Commands::Url(args) = cli.command {
        assert_eq!(args.service(), Some("db"));
        assert_eq!(args.format, "json".to_string());
        assert_eq!(args.kind(), Some(config::Kind::Database));
        assert_eq!(args.driver(), Some(config::Driver::Mysql));
    } else {
        panic!("expected url command");
    }

    let cli = Cli::parse_from([
        "stackctl",
        "setup",
        "--service",
        "api",
        "--kind",
        "app",
        "--parallel",
        "2",
    ]);
    if let commands::Commands::Setup(args) = cli.command {
        assert_eq!(args.service(), Some("api"));
        assert_eq!(args.kind(), Some(config::Kind::App));
        assert_eq!(args.parallel, 2);
    } else {
        panic!("expected setup command");
    }

    let cli = Cli::parse_from([
        "stackctl",
        "start",
        "--profile",
        "full",
        "--wait",
        "--parallel",
        "4",
    ]);
    if let commands::Commands::Start(args) = cli.command {
        assert_eq!(args.profile(), Some("full"));
        assert!(args.wait);
        assert!(!args.no_wait);
    } else {
        panic!("expected start command");
    }

    let cli = Cli::parse_from([
        "stackctl",
        "up",
        "--service",
        "worker",
        "--kind",
        "app",
        "--publish-all",
        "--port-strategy",
        "stable",
        "--port-seed",
        "seed",
        "--parallel",
        "3",
    ]);
    if let commands::Commands::Up(args) = cli.command {
        assert_eq!(args.service(), Some("worker"));
        assert_eq!(args.kind(), Some(config::Kind::App));
        assert_eq!(args.port_seed(), Some("seed"));
        assert!(args.publish_all);
        assert_eq!(args.parallel, 3);
    } else {
        panic!("expected up command");
    }

    let cli = Cli::parse_from(["stackctl", "apply", "--no-deps"]);
    if let commands::Commands::Apply(args) = cli.command {
        assert!(args.no_deps);
    } else {
        panic!("expected apply command");
    }

    let cli = Cli::parse_from([
        "stackctl",
        "update",
        "--profile",
        "default",
        "--force-recreate",
        "--no-build",
    ]);
    if let commands::Commands::Update(args) = cli.command {
        assert_eq!(args.profile(), Some("default"));
        assert!(args.force_recreate);
        assert!(args.no_build);
    } else {
        panic!("expected update command");
    }
}

#[test]
fn operations_argument_methods() {
    let cli = Cli::parse_from([
        "stackctl",
        "restore",
        "--service",
        "mysql",
        "--file",
        "/tmp/backup.sql",
        "--reset",
        "--migrate",
        "--schema-dump",
        "--gzip",
    ]);
    if let commands::Commands::Restore(args) = cli.command {
        assert_eq!(args.service(), Some("mysql"));
        assert_eq!(args.file, Some(PathBuf::from("/tmp/backup.sql")));
        assert!(args.reset);
        assert!(args.migrate);
        assert!(args.schema_dump);
        assert!(args.gzip);
    } else {
        panic!("expected restore command");
    }

    let cli = Cli::parse_from([
        "stackctl",
        "dump",
        "--service",
        "mysql",
        "--file",
        "/tmp/dump.sql",
        "--stdout",
        "--gzip",
    ]);
    if let commands::Commands::Dump(args) = cli.command {
        assert_eq!(args.service(), Some("mysql"));
        assert_eq!(args.file, Some(PathBuf::from("/tmp/dump.sql")));
        assert!(args.stdout);
        assert!(args.gzip);
    } else {
        panic!("expected dump command");
    }

    let cli = Cli::parse_from([
        "stackctl",
        "pull",
        "--service",
        "api",
        "--kind",
        "app",
        "--parallel",
        "3",
    ]);
    if let commands::Commands::Pull(args) = cli.command {
        assert_eq!(args.service(), Some("api"));
        assert_eq!(args.kind(), Some(config::Kind::App));
        assert_eq!(args.parallel, 3);
    } else {
        panic!("expected pull command");
    }

    let cli = Cli::parse_from(["stackctl", "about"]);
    if let commands::Commands::About(_args) = cli.command {
        // AboutArgs is intentionally empty and uses default behavior.
    } else {
        panic!("expected about command");
    }

    let cli = Cli::parse_from([
        "stackctl", "ps", "--format", "json", "--kind", "app", "--driver", "postgres",
    ]);
    if let commands::Commands::Ps(args) = cli.command {
        assert_eq!(args.kind(), Some(config::Kind::App));
        assert_eq!(args.driver(), Some(config::Driver::Postgres));
    } else {
        panic!("expected ps command");
    }

    let cli = Cli::parse_from([
        "stackctl",
        "health",
        "--service",
        "api",
        "--kind",
        "app",
        "--timeout",
        "10",
        "--interval",
        "1",
        "--retries",
        "3",
        "--parallel",
        "2",
    ]);
    if let commands::Commands::Health(args) = cli.command {
        assert_eq!(args.service(), Some("api"));
        assert_eq!(args.kind(), Some(config::Kind::App));
        assert_eq!(args.timeout, 10);
    } else {
        panic!("expected health command");
    }

    let cli = Cli::parse_from([
        "stackctl",
        "env",
        "--service",
        "api",
        "--kind",
        "app",
        "--env-file",
        "/tmp/env-file",
        "--sync",
        "--purge",
        "--persist-runtime",
        "--create-missing",
        "generate",
        "--output",
        "/tmp/env",
    ]);
    if let commands::Commands::Env(args) = cli.command {
        assert_eq!(args.service(), Some("api"));
        assert_eq!(args.kind(), Some(config::Kind::App));
        assert!(matches!(args.command, Some(EnvCommands::Generate { .. })));
        assert_eq!(args.env_file, Some(PathBuf::from("/tmp/env-file")));
        assert!(args.sync);
        assert!(args.purge);
        assert!(args.persist_runtime);
        assert!(args.create_missing);
    } else {
        panic!("expected env command");
    }

    let cli = Cli::parse_from([
        "stackctl", "ls", "--kind", "cache", "--driver", "redis", "--format", "json",
    ]);
    if let commands::Commands::Ls(args) = cli.command {
        assert_eq!(args.kind(), Some(config::Kind::Cache));
        assert_eq!(args.driver(), Some(config::Driver::Redis));
    } else {
        panic!("expected ls command");
    }

    let cli = Cli::parse_from([
        "stackctl",
        "attach",
        "--service",
        "api",
        "--no-stdin",
        "--sig-proxy",
        "--detach-keys",
        "ctrl-c",
    ]);
    if let commands::Commands::Attach(args) = cli.command {
        assert_eq!(args.service(), Some("api"));
        assert!(args.no_stdin);
        assert!(args.sig_proxy);
        assert_eq!(args.detach_keys(), Some("ctrl-c"));
    } else {
        panic!("expected attach command");
    }

    let cli = Cli::parse_from(["stackctl", "cp", "-L", "src", "dst"]);
    if let commands::Commands::Cp(args) = cli.command {
        assert!(args.follow_link);
        assert!(!args.archive);
        assert_eq!(args.source, "src");
        assert_eq!(args.destination, "dst");
    } else {
        panic!("expected cp command");
    }

    let cli = Cli::parse_from([
        "stackctl",
        "events",
        "--service",
        "db",
        "--since",
        "1m",
        "--until",
        "2m",
        "--format",
        "json",
        "--filter",
        "type=container",
    ]);
    if let commands::Commands::Events(args) = cli.command {
        assert_eq!(args.service(), Some("db"));
        assert_eq!(args.since(), Some("1m"));
        assert_eq!(args.until(), Some("2m"));
        assert_eq!(args.format(), Some("json"));
    } else {
        panic!("expected events command");
    }

    let cli = Cli::parse_from([
        "stackctl",
        "inspect",
        "--service",
        "cache",
        "--kind",
        "cache",
        "--format",
        "json",
        "--size",
        "--type",
        "container",
    ]);
    if let commands::Commands::Inspect(args) = cli.command {
        assert_eq!(args.service(), Some("cache"));
        assert_eq!(args.kind(), Some(config::Kind::Cache));
        assert_eq!(args.format(), Some("json"));
        assert_eq!(args.object_type(), Some("container"));
    } else {
        panic!("expected inspect command");
    }

    let cli = Cli::parse_from([
        "stackctl",
        "kill",
        "--service",
        "cache",
        "--kind",
        "cache",
        "--signal",
        "SIGTERM",
        "--parallel",
        "2",
    ]);
    if let commands::Commands::Kill(args) = cli.command {
        assert_eq!(args.service(), Some("cache"));
        assert_eq!(args.kind(), Some(config::Kind::Cache));
        assert_eq!(args.signal(), Some("SIGTERM"));
    } else {
        panic!("expected kill command");
    }

    let cli = Cli::parse_from([
        "stackctl",
        "pause",
        "--service",
        "cache",
        "--kind",
        "cache",
        "--parallel",
        "2",
    ]);
    if let commands::Commands::Pause(args) = cli.command {
        assert_eq!(args.service(), Some("cache"));
        assert_eq!(args.kind(), Some(config::Kind::Cache));
    } else {
        panic!("expected pause command");
    }

    let cli = Cli::parse_from([
        "stackctl",
        "unpause",
        "--service",
        "api",
        "--kind",
        "app",
        "--parallel",
        "2",
    ]);
    if let commands::Commands::Unpause(args) = cli.command {
        assert_eq!(args.service(), Some("api"));
        assert_eq!(args.kind(), Some(config::Kind::App));
    } else {
        panic!("expected unpause command");
    }

    let cli = Cli::parse_from([
        "stackctl",
        "stats",
        "--service",
        "api",
        "--kind",
        "app",
        "--no-stream",
        "--format",
        "json",
    ]);
    if let commands::Commands::Stats(args) = cli.command {
        assert_eq!(args.service(), Some("api"));
        assert!(args.no_stream);
    } else {
        panic!("expected stats command");
    }

    let cli = Cli::parse_from([
        "stackctl",
        "top",
        "--service",
        "api",
        "--kind",
        "app",
        "--",
        "-eo",
        "pid",
    ]);
    if let commands::Commands::Top(args) = cli.command {
        assert_eq!(args.service(), Some("api"));
        assert!(args.args.len() >= 2);
    } else {
        panic!("expected top command");
    }

    let cli = Cli::parse_from([
        "stackctl",
        "wait",
        "--service",
        "api",
        "--kind",
        "app",
        "--condition",
        "running",
        "--parallel",
        "2",
    ]);
    if let commands::Commands::Wait(args) = cli.command {
        assert_eq!(args.service(), Some("api"));
        assert_eq!(args.condition(), Some("running"));
        assert_eq!(args.parallel, 2);
    } else {
        panic!("expected wait command");
    }

    let cli = Cli::parse_from([
        "stackctl",
        "logs",
        "--all",
        "--prefix",
        "--follow",
        "--tail",
        "25",
        "--timestamps",
        "--access",
    ]);
    if let commands::Commands::Logs(args) = cli.command {
        assert!(args.all);
        assert_eq!(args.tail, Some(25));
        assert!(args.prefix);
        assert!(args.follow);
        assert!(args.timestamps);
        assert!(args.access);
    } else {
        panic!("expected logs command");
    }

    let cli = Cli::parse_from([
        "stackctl",
        "swarm",
        "--force",
        "--parallel",
        "3",
        "--fail-fast",
        "--port-strategy",
        "random",
        "--port-seed",
        "seed",
        "--env-output",
        "up",
    ]);
    if let commands::Commands::Swarm(args) = cli.command {
        assert!(args.force);
        assert_eq!(args.parallel, 3);
        assert!(args.fail_fast);
        assert_eq!(args.port_seed(), Some("seed"));
        assert!(args.env_output);
        assert!(!args.command.is_empty());
    } else {
        panic!("expected swarm command");
    }

    let cli = Cli::parse_from([
        "stackctl",
        "port",
        "--service",
        "api",
        "--kind",
        "app",
        "--json",
        "80/tcp",
    ]);
    if let commands::Commands::Port(args) = cli.command {
        assert_eq!(args.service(), Some("api"));
        assert!(args.json);
        assert_eq!(args.private_port(), Some("80/tcp"));
    } else {
        panic!("expected port command");
    }

    let cli = Cli::parse_from([
        "stackctl",
        "prune",
        "--all",
        "--force",
        "--filter",
        "label=keep",
    ]);
    if let commands::Commands::Prune(args) = cli.command {
        assert!(args.all);
        assert!(args.force);
        assert_eq!(args.filter, vec!["label=keep".to_owned()]);
    } else {
        panic!("expected prune command");
    }
}

#[test]
fn command_variants_parse() {
    let completion = Cli::parse_from(["stackctl", "completions", "bash"]);
    if let commands::Commands::Completions(args) = completion.command {
        assert_eq!(args.shell, clap_complete::Shell::Bash);
    } else {
        panic!("expected completions command");
    }

    let doctor = Cli::parse_from(["stackctl", "doctor", "--repro"]);
    if let commands::Commands::Doctor(args) = doctor.command {
        assert!(args.repro);
    } else {
        panic!("expected doctor command");
    }

    let preset = Cli::parse_from(["stackctl", "preset", "show", "app", "--format", "toml"]);
    if let commands::Commands::Preset(commands::PresetArgs { command }) = preset.command {
        assert!(
            matches!(command, PresetCommands::Show { name, format } if name == "app" && format == "toml")
        );
    } else {
        panic!("expected preset command");
    }

    let profile = Cli::parse_from([
        "stackctl", "profile", "show", "default", "--format", "table",
    ]);
    if let commands::Commands::Profile(commands::ProfileArgs { command }) = profile.command {
        assert!(
            matches!(command, ProfileCommands::Show { name, format } if name == "default" && format == "table")
        );
    } else {
        panic!("expected profile command");
    }

    let config = Cli::parse_from(["stackctl", "config", "migrate", "--to", "yaml"]);
    if let commands::Commands::Config(commands::ConfigArgs { command, .. }) = config.command {
        assert!(matches!(command, Some(ConfigCommands::Migrate { to }) if to == "yaml"));
    } else {
        panic!("expected config command");
    }

    let config_schema = Cli::parse_from(["stackctl", "config", "schema"]);
    if let commands::Commands::Config(commands::ConfigArgs { command, .. }) = config_schema.command
    {
        assert!(matches!(command, Some(ConfigCommands::Schema)));
    } else {
        panic!("expected config schema command");
    }

    let config_validate = Cli::parse_from([
        "stackctl",
        "config",
        "validate",
        "/work/bill/.stackctl.yaml",
    ]);
    if let commands::Commands::Config(commands::ConfigArgs { command, .. }) =
        config_validate.command
    {
        assert!(matches!(
            command,
            Some(ConfigCommands::Validate { path })
                if path == Some(PathBuf::from("/work/bill/.stackctl.yaml"))
        ));
    } else {
        panic!("expected config validate command");
    }

    let env = Cli::parse_from(["stackctl", "env", "generate", "--output", "/tmp/env-out"]);
    if let commands::Commands::Env(commands::EnvArgs { command, .. }) = env.command {
        assert!(
            matches!(command, Some(EnvCommands::Generate { output }) if output == PathBuf::from("/tmp/env-out"))
        );
    } else {
        panic!("expected env command");
    }

    let lock = Cli::parse_from(["stackctl", "lock", "verify"]);
    if let commands::Commands::Lock(commands::LockArgs { command }) = lock.command {
        assert!(matches!(command, LockCommands::Verify));
    } else {
        panic!("expected lock command");
    }
}

#[test]
fn share_command_variants_parse() {
    let status = Cli::parse_from(["stackctl", "share", "status"]);
    if let commands::Commands::Share(args) = status.command {
        assert!(matches!(args.command, commands::ShareCommands::Status(_)));
    } else {
        panic!("expected share command");
    }

    let start = Cli::parse_from(["stackctl", "share", "start", "--provider", "cloudflare"]);
    if let commands::Commands::Share(args) = start.command {
        assert!(matches!(args.command, commands::ShareCommands::Start(_)));
    } else {
        panic!("expected share command");
    }

    let stop = Cli::parse_from(["stackctl", "share", "stop", "--all"]);
    if let commands::Commands::Share(args) = stop.command {
        assert!(matches!(args.command, commands::ShareCommands::Stop(_)));
    } else {
        panic!("expected share command");
    }
}

#[test]
fn daemon_command_variants_parse() {
    let watch = Cli::parse_from(["stackctl", "daemon", "watch", "--dir", "/tmp/projects"]);
    if let commands::Commands::Daemon(args) = watch.command {
        assert!(matches!(args.command, commands::DaemonCommands::Watch(_)));
    } else {
        panic!("expected daemon command");
    }

    let service = Cli::parse_from([
        "stackctl",
        "daemon",
        "service",
        "install",
        "--dir",
        "/tmp/projects",
    ]);
    if let commands::Commands::Daemon(args) = service.command {
        assert!(matches!(args.command, commands::DaemonCommands::Service(_)));
    } else {
        panic!("expected daemon command");
    }

    let status = Cli::parse_from(["stackctl", "daemon", "status"]);
    if let commands::Commands::Daemon(args) = status.command {
        assert!(matches!(args.command, commands::DaemonCommands::Status));
    } else {
        panic!("expected daemon command");
    }

    let reconcile = Cli::parse_from(["stackctl", "daemon", "reconcile"]);
    if let commands::Commands::Daemon(args) = reconcile.command {
        assert!(matches!(args.command, commands::DaemonCommands::Reconcile));
    } else {
        panic!("expected daemon command");
    }

    let benchmark = Cli::parse_from(["stackctl", "daemon", "benchmark"]);
    if let commands::Commands::Daemon(args) = benchmark.command {
        assert!(matches!(args.command, commands::DaemonCommands::Benchmark));
    } else {
        panic!("expected daemon command");
    }

    let adopt = Cli::parse_from(["stackctl", "daemon", "adopt", "/work/bill"]);
    if let commands::Commands::Daemon(args) = adopt.command {
        let commands::DaemonCommands::Adopt(args) = args.command else {
            panic!("expected daemon adopt command");
        };
        assert_eq!(args.path, PathBuf::from("/work/bill"));
    } else {
        panic!("expected daemon command");
    }

    let migration = Cli::parse_from(["stackctl", "daemon", "migration", "status", "/work/bill"]);
    if let commands::Commands::Daemon(args) = migration.command {
        let commands::DaemonCommands::Migration(args) = args.command else {
            panic!("expected daemon migration command");
        };
        let commands::DaemonMigrationCommands::Status(args) = args.command else {
            panic!("expected daemon migration status command");
        };
        assert_eq!(args.path, PathBuf::from("/work/bill"));
    } else {
        panic!("expected daemon command");
    }

    for command in ["confirm", "rollback"] {
        let migration = Cli::parse_from([
            "stackctl",
            "daemon",
            "migration",
            command,
            "restore-42",
            "/work/bill",
        ]);
        let commands::Commands::Daemon(args) = migration.command else {
            panic!("expected daemon command");
        };
        let commands::DaemonCommands::Migration(args) = args.command else {
            panic!("expected daemon migration command");
        };
        let args = match args.command {
            commands::DaemonMigrationCommands::Confirm(args) if command == "confirm" => args,
            commands::DaemonMigrationCommands::Rollback(args) if command == "rollback" => args,
            _ => panic!("expected daemon migration {command} command"),
        };
        assert_eq!(args.migration_id, "restore-42");
        assert_eq!(args.path, PathBuf::from("/work/bill"));
    }

    let backup = Cli::parse_from(["stackctl", "daemon", "backup", "database", "/work/bill"]);
    if let commands::Commands::Daemon(args) = backup.command {
        let commands::DaemonCommands::Backup(args) = args.command else {
            panic!("expected daemon backup command");
        };
        assert_eq!(args.service, "database");
        assert_eq!(args.path, PathBuf::from("/work/bill"));
    } else {
        panic!("expected daemon command");
    }

    let backups = Cli::parse_from(["stackctl", "daemon", "backups", "/work/bill"]);
    if let commands::Commands::Daemon(args) = backups.command {
        let commands::DaemonCommands::Backups(args) = args.command else {
            panic!("expected daemon backups command");
        };
        assert_eq!(args.path, PathBuf::from("/work/bill"));
    } else {
        panic!("expected daemon command");
    }

    let restore = Cli::parse_from(["stackctl", "daemon", "restore", "backup-42", "/work/bill"]);
    if let commands::Commands::Daemon(args) = restore.command {
        let commands::DaemonCommands::Restore(args) = args.command else {
            panic!("expected daemon restore command");
        };
        assert_eq!(args.recovery_point_id, "backup-42");
        assert_eq!(args.path, PathBuf::from("/work/bill"));
    } else {
        panic!("expected daemon command");
    }

    let prune = Cli::parse_from([
        "stackctl",
        "daemon",
        "prune",
        "plan",
        "bill",
        "database",
        "backup-42",
    ]);
    if let commands::Commands::Daemon(args) = prune.command {
        let commands::DaemonCommands::Prune(args) = args.command else {
            panic!("expected daemon prune command");
        };
        let commands::DaemonPruneCommands::Plan(args) = args.command else {
            panic!("expected daemon prune plan command");
        };
        assert_eq!(args.project_id, "bill");
        assert_eq!(args.service_id, "database");
        assert_eq!(args.recovery_point_id, "backup-42");
    } else {
        panic!("expected daemon command");
    }

    let execute = Cli::parse_from([
        "stackctl",
        "daemon",
        "prune",
        "execute",
        "bill",
        "database",
        "backup-42",
        "--confirmation-token",
        "token-42",
    ]);
    if let commands::Commands::Daemon(args) = execute.command {
        let commands::DaemonCommands::Prune(args) = args.command else {
            panic!("expected daemon prune command");
        };
        let commands::DaemonPruneCommands::Execute(args) = args.command else {
            panic!("expected daemon prune execute command");
        };
        assert_eq!(args.project_id, "bill");
        assert_eq!(args.service_id, "database");
        assert_eq!(args.recovery_point_id, "backup-42");
        assert_eq!(args.confirmation_token, "token-42");
    } else {
        panic!("expected daemon command");
    }
}

#[test]
fn singleton_daemon_rejects_partial_registry_watch_flags() {
    assert!(
        Cli::try_parse_from([
            "stackctl",
            "daemon",
            "watch",
            "--dir",
            "/tmp/projects",
            "--exclude-dir",
            "/tmp/archive",
        ])
        .is_err()
    );
    assert!(
        Cli::try_parse_from([
            "stackctl",
            "daemon",
            "service",
            "install",
            "--dir",
            "/tmp/projects",
            "--max-projects",
            "5",
        ])
        .is_err()
    );
}
