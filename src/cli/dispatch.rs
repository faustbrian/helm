//! Direct strict-v8 CLI dispatch.

use anyhow::{Result, bail};
use clap::CommandFactory;
use clap_complete::generate;

use super::args::{Cli, Commands, ConfigCommands};
use super::handlers;
use crate::output;

pub(crate) mod context;

/// Executes one strict v8 CLI invocation.
pub(crate) fn run(cli: Cli) -> Result<()> {
    if cli.no_color {
        colored::control::set_override(false);
    }
    output::init(cli.quiet);
    let context = context::CliDispatchContext::from_cli(&cli);

    match &cli.command {
        Commands::Config(args) => {
            return match &args.command {
                ConfigCommands::Schema => handlers::handle_config_schema(),
                ConfigCommands::Validate { path } => {
                    handlers::handle_config_validate(path.as_deref(), &context)
                }
            };
        }
        Commands::Completions(args) => {
            let mut command = Cli::command();
            generate(args.shell, &mut command, "stackctl", &mut std::io::stdout());
            return Ok(());
        }
        Commands::Daemon(args) => return handlers::handle_daemon(args),
        Commands::Doctor(args) => return handlers::handle_doctor(args),
        Commands::Setup(args) => return handlers::handle_setup(args),
        _ => {}
    }

    #[cfg(unix)]
    if handlers::handle_v8_env(&cli, &context)?
        || handlers::handle_v8_status(&cli, &context)?
        || handlers::handle_v8_url(&cli, &context)?
        || handlers::handle_v8_open(&cli, &context)?
        || handlers::handle_v8_logs(&cli, &context)?
        || handlers::handle_v8_lock(&cli, &context)?
        || handlers::handle_v8_workflow(&cli, &context)?
        || handlers::handle_v8_project_command(&cli, &context)?
    {
        return Ok(());
    }

    bail!("this v8 command requires a strict .stackctl.yaml project")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use clap::Parser;

    use crate::cli::args::Cli;

    #[test]
    fn unrelated_project_files_are_not_treated_as_stackctl_configuration() {
        let root = std::env::temp_dir().join(format!(
            "stackctl-v8-dispatch-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create project root");
        fs::write(root.join("project.toml"), "schema_version = 1\n")
            .expect("write unrelated config");

        let error = super::run(Cli::parse_from([
            "stackctl",
            "--project-root",
            root.to_str().expect("root"),
            "status",
        ]))
        .expect_err("missing v8 config");

        assert_eq!(
            error.to_string(),
            "this v8 command requires a strict .stackctl.yaml project"
        );
    }

    #[test]
    fn config_validate_resolves_v8_yaml_offline_without_modifying_it() {
        let root = std::env::temp_dir().join(format!(
            "stackctl-v8-validate-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create project directory");
        let path = root.join(".stackctl.yaml");
        let source = "schema_version: 8\nservices:\n  app:\n    preset: laravel\n";
        fs::write(&path, source).expect("write v8 config");

        super::run(Cli::parse_from([
            "stackctl",
            "config",
            "validate",
            path.to_str().expect("config path"),
        ]))
        .expect("valid v8 config");

        assert_eq!(fs::read_to_string(path).expect("reread config"), source);
    }
}
