//! swarm targets bootstrap module.
//!
//! Contains swarm targets bootstrap logic used by Helm command workflows.

use anyhow::Result;

use super::ResolvedSwarmTarget;
use clone::clone_missing_root;

mod clone;

pub(super) fn bootstrap_swarm_targets(
    config: &crate::config::Config,
    targets: &[ResolvedSwarmTarget],
    quiet: bool,
) -> Result<()> {
    for target in targets {
        if target.root.exists() {
            continue;
        }

        let Some(configured_target) = config.swarm.iter().find(|item| item.name == target.name)
        else {
            anyhow::bail!("swarm target '{}' missing from config", target.name);
        };

        let Some(git) = configured_target.git.as_ref() else {
            continue;
        };

        clone_missing_root(&target.name, &target.root, git, quiet)?;
    }

    ensure_target_configs_exist(targets)
}

/// Ensures target configs exist exists and is in the required state.
pub(super) fn ensure_target_configs_exist(targets: &[ResolvedSwarmTarget]) -> Result<()> {
    for target in targets {
        let target_config = target.root.join(".helm.toml");
        if !target_config.exists() {
            anyhow::bail!(
                "missing .helm.toml for swarm target '{}' at {}",
                target.name,
                target.root.display()
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Config, SwarmGit, SwarmTarget};
    use anyhow::Context;
    use std::path::Path;
    use std::process::Command;

    #[test]
    fn bootstrap_swarm_targets_clones_missing_roots_with_git_config() -> Result<()> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let base = std::env::temp_dir().join(format!("helm-swarm-bootstrap-{nonce}"));
        let source = base.join("source");
        let workspace = base.join("workspace");
        let target_root = workspace.join("rate");

        std::fs::create_dir_all(&source)?;
        std::fs::create_dir_all(&workspace)?;

        run_git(["init", "--initial-branch=main"], &source)?;
        run_git(["config", "user.email", "tests@example.com"], &source)?;
        run_git(["config", "user.name", "Helm Tests"], &source)?;
        std::fs::write(source.join(".helm.toml"), "container_prefix = \"rate\"\n")?;
        run_git(["add", ".helm.toml"], &source)?;
        run_git(["commit", "-m", "init"], &source)?;
        run_git(["branch", "develop"], &source)?;

        let source_repo = source.to_string_lossy().into_owned();
        let config = Config {
            schema_version: 1,
            container_prefix: None,
            service: Vec::new(),
            swarm: vec![SwarmTarget {
                name: "rate".to_owned(),
                root: target_root.clone(),
                depends_on: vec![],
                inject_env: vec![],
                git: Some(SwarmGit {
                    repo: source_repo,
                    branch: Some("develop".to_owned()),
                }),
            }],
        };
        let targets = vec![ResolvedSwarmTarget {
            name: "rate".to_owned(),
            root: target_root.clone(),
        }];

        bootstrap_swarm_targets(&config, &targets, false)?;

        assert!(target_root.exists());
        assert!(target_root.join(".helm.toml").exists());

        std::fs::remove_dir_all(base)?;
        Ok(())
    }

    fn run_git<const N: usize>(args: [&str; N], cwd: &Path) -> Result<()> {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .with_context(|| format!("failed to execute git {}", args.join(" ")))?;
        if output.status.success() {
            return Ok(());
        }
        anyhow::bail!(
            "git command failed in {}: {}",
            cwd.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
