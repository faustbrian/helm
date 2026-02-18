//! database post restore module.
//!
//! Contains database post restore logic used by Helm command workflows.

use anyhow::{Context, Result};
use std::path::Path;

use crate::output::{self, LogLevel, Persistence};

/// Runs optional post-restore artisan hooks for the active app service.
pub(crate) fn run_laravel_post_restore(
    run_migrate: bool,
    run_schema_dump: bool,
    project_root_override: Option<&Path>,
    config_path: Option<&Path>,
) -> Result<()> {
    let project_root = crate::config::project_root_with(
        crate::config::ProjectRootPathOptions::new(config_path, project_root_override),
    )?;

    if run_migrate {
        run_artisan_command(&project_root, "migrate")?;
    }

    if run_schema_dump {
        run_artisan_command(&project_root, "schema:dump")?;
    }

    Ok(())
}

/// Runs a single artisan command via `helm artisan`.
fn run_artisan_command(project_root: &Path, artisan_command: &str) -> Result<()> {
    output::event(
        "database",
        LogLevel::Info,
        &running_message(artisan_command),
        Persistence::Persistent,
    );

    if crate::docker::is_dry_run() {
        output::event(
            "database",
            LogLevel::Info,
            &dry_run_message(artisan_command),
            Persistence::Transient,
        );
        return Ok(());
    }

    let config = crate::config::load_config_with(crate::config::LoadConfigPathOptions::new(
        None,
        Some(project_root),
    ))
    .context("failed to load helm config for artisan post-restore hook")?;
    let app_service = crate::config::resolve_app_service(&config, None)
        .context("failed to resolve app service for artisan post-restore hook")?;
    let args = vec![artisan_command.to_owned()];
    crate::serve::exec_artisan(app_service, &args, false)
        .context("Failed to execute helm artisan command")?;

    output::event(
        "database",
        LogLevel::Success,
        &completed_message(artisan_command),
        Persistence::Persistent,
    );
    Ok(())
}

fn running_message(artisan_command: &str) -> String {
    format!("Running `helm artisan {artisan_command}`")
}

fn dry_run_message(artisan_command: &str) -> String {
    format!("[dry-run] helm artisan {artisan_command}")
}

fn completed_message(artisan_command: &str) -> String {
    format!("`helm artisan {artisan_command}` completed")
}

#[cfg(test)]
mod tests {
    use super::run_laravel_post_restore;
    use super::{completed_message, dry_run_message, running_message};
    use std::env;
    use std::fs;
    use std::io::Write;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn with_temp_project<F, T>(test: F) -> T
    where
        F: FnOnce(&Path) -> T,
    {
        let root = env::temp_dir().join(format!(
            "helm-post-restore-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temp project");
        fs::write(
            root.join(".helm.toml"),
            "schema_version = 1\ncontainer_prefix = \"acme\"\n[[service]]\npreset = \"laravel\"\nname = \"app\"\n",
        )
        .expect("write temp config");

        let result = test(&root);
        fs::remove_dir_all(&root).ok();
        result
    }

    fn with_fake_docker_command<F, T>(script: &str, test: F) -> T
    where
        F: FnOnce() -> T,
    {
        let bin_dir = env::temp_dir().join(format!(
            "helm-post-restore-docker-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&bin_dir).expect("create fake docker dir");
        let binary = bin_dir.join("docker");
        let mut file = fs::File::create(&binary).expect("create fake docker");
        writeln!(file, "#!/bin/sh\n{}", script).expect("write fake docker");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&binary)
                .expect("fake docker metadata")
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&binary, perms).expect("chmod fake docker");
        }

        let command = binary.to_string_lossy().to_string();
        let result = crate::docker::with_docker_command(&command, test);
        fs::remove_dir_all(&bin_dir).ok();
        result
    }

    #[test]
    fn run_laravel_post_restore_supports_partial_hook_combinations() {
        with_temp_project(|project_root| {
            crate::docker::with_dry_run_lock(|| {
                run_laravel_post_restore(false, false, Some(project_root), None)
                    .expect("post-restore skip path");
            });
        });
    }

    #[test]
    fn run_laravel_post_restore_executes_post_restore_with_mocked_docker() {
        with_temp_project(|project_root| {
            with_fake_docker_command("exit 0", || {
                run_laravel_post_restore(true, true, Some(project_root), None)
                    .expect("post-restore run");
            });
        });
    }

    #[test]
    fn run_laravel_post_restore_executes_schema_dump_with_mocked_docker() {
        with_temp_project(|project_root| {
            with_fake_docker_command("exit 0", || {
                run_laravel_post_restore(false, true, Some(project_root), None)
                    .expect("schema dump post-restore");
            });
        });
    }

    #[test]
    fn run_laravel_post_restore_dry_run_only_skips_artisan_execution() {
        with_temp_project(|project_root| {
            crate::docker::with_dry_run_lock(|| {
                run_laravel_post_restore(true, true, Some(project_root), None)
                    .expect("dry-run post-restore");
            });
        });
    }

    #[test]
    fn message_helpers_serialize_expected_artisan_labels() {
        assert_eq!(running_message("migrate"), "Running `helm artisan migrate`");
        assert_eq!(
            dry_run_message("schema:dump"),
            "[dry-run] helm artisan schema:dump"
        );
        assert_eq!(
            completed_message("config:cache"),
            "`helm artisan config:cache` completed"
        );
    }
}
