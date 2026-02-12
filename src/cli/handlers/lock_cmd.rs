use anyhow::Result;
use std::path::Path;

use crate::cli::args::LockCommands;
use crate::config;

pub(crate) fn handle_lock(
    config_data: &config::Config,
    command: &LockCommands,
    quiet: bool,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<()> {
    match command {
        LockCommands::Images => {
            let lockfile = config::build_image_lock(config_data)?;
            let path = config::save_lockfile_with(&lockfile, config_path, project_root)?;
            if !quiet {
                println!(
                    "Wrote {} with {} image entries",
                    path.display(),
                    lockfile.images.len()
                );
            }
            Ok(())
        }
        LockCommands::Verify => {
            let expected = config::build_image_lock(config_data)?;
            let actual = config::load_lockfile_with(config_path, project_root)?;
            let diff = config::lockfile_diff(&expected, &actual);

            if diff.missing.is_empty() && diff.changed.is_empty() && diff.extra.is_empty() {
                if !quiet {
                    println!("Lockfile is in sync");
                }
                return Ok(());
            }

            print_diff(&diff);
            anyhow::bail!("lockfile is out of sync; run `helm lock images`")
        }
        LockCommands::Diff => {
            let expected = config::build_image_lock(config_data)?;
            let actual = config::load_lockfile_with(config_path, project_root)
                .unwrap_or_else(|_| config::Lockfile::default());
            let diff = config::lockfile_diff(&expected, &actual);
            print_diff(&diff);
            Ok(())
        }
    }
}

fn print_diff(diff: &config::LockfileDiff) {
    if diff.missing.is_empty() && diff.changed.is_empty() && diff.extra.is_empty() {
        println!("No lockfile changes");
        return;
    }

    for item in &diff.missing {
        println!("+ {} {} -> {}", item.service, item.image, item.resolved);
    }
    for (expected, actual) in &diff.changed {
        println!(
            "~ {} {} -> {} (was {})",
            expected.service, expected.image, expected.resolved, actual.resolved
        );
    }
    for item in &diff.extra {
        println!("- {} {} -> {}", item.service, item.image, item.resolved);
    }
}
