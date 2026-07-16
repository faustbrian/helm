use anyhow::{Context, Result};
use std::path::Path;

use crate::cli::dispatch::context::CliDispatchContext;

/// Validates complete v8 desired state without contacting mutable runtime state.
pub(crate) fn handle_config_validate(
    path: Option<&Path>,
    context: &CliDispatchContext<'_>,
) -> Result<()> {
    let path = match path {
        Some(path) => path.to_path_buf(),
        None => context
            .config_path()
            .map(Path::to_path_buf)
            .or_else(|| {
                context
                    .project_root()
                    .map(|root| root.join(".stackctl.yaml"))
            })
            .map_or_else(
                || std::env::current_dir().map(|root| root.join(".stackctl.yaml")),
                Ok,
            )?,
    };
    let source = crate::control_plane::read_bounded_yaml_file(
        &path,
        crate::control_plane::MAX_PROJECT_CONFIG_BYTES,
    )
    .with_context(|| format!("failed to read {}", path.display()))?;
    let raw = crate::control_plane::parse_project_config(&source, &path)?;
    let project_directory = path
        .parent()
        .context("v8 configuration path must have a project directory")?;
    crate::control_plane::resolve_desired_project(raw, project_directory)
        .with_context(|| format!("invalid desired state in {}", path.display()))?;

    if !context.quiet() {
        println!("valid: {}", path.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use clap::Parser;

    use crate::cli::args::Cli;
    use crate::cli::dispatch::context::CliDispatchContext;

    #[test]
    fn validation_honors_the_global_project_root() {
        let root = std::env::temp_dir().join(format!(
            "stackctl-config-validation-project-root-{}",
            std::process::id()
        ));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create project root");
        fs::write(
            root.join(".stackctl.yaml"),
            "schema_version: 8\nproject: acceptance\nservices:\n  app:\n    preset: laravel\n",
        )
        .expect("write project configuration");
        let root_argument = root.to_string_lossy().into_owned();
        let cli = Cli::parse_from([
            "stackctl",
            "--quiet",
            "--project-root",
            root_argument.as_str(),
            "config",
            "validate",
        ]);
        let context = CliDispatchContext::from_cli(&cli);

        super::handle_config_validate(None, &context).expect("validate project-root config");

        fs::remove_dir_all(root).expect("remove project root");
    }
}
