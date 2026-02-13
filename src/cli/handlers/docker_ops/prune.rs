//! cli handlers docker ops prune module.
//!
//! Contains prune handler used by Helm command workflows.

use anyhow::Result;

use crate::output::{self, LogLevel, Persistence};
use crate::{cli, config, docker};

pub(super) fn handle_prune(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    parallel: usize,
    all: bool,
    force: bool,
    filter: &[String],
) -> Result<()> {
    validate_prune_args(all, force, filter)?;

    if all {
        output::event(
            "docker",
            LogLevel::Warn,
            "Pruning all stopped Docker containers via --all",
            Persistence::Persistent,
        );
        return docker::prune(force, filter);
    }

    cli::support::for_each_service(config, service, kind, None, parallel, |svc| {
        docker::prune_stopped_container(svc)
    })
}

fn validate_prune_args(all: bool, force: bool, filter: &[String]) -> Result<()> {
    if all && !force {
        anyhow::bail!("--all requires --force to avoid accidental global prune");
    }
    if force && !all {
        anyhow::bail!("--force can only be used with --all");
    }
    if !filter.is_empty() && !all {
        anyhow::bail!("--filter can only be used with --all");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_prune_args;

    #[test]
    fn global_prune_requires_force() {
        let err = validate_prune_args(true, false, &[]).expect_err("expected validation failure");
        assert!(err.to_string().contains("--all requires --force"));
    }

    #[test]
    fn local_prune_rejects_force_and_filter() {
        assert!(validate_prune_args(false, true, &[]).is_err());
        assert!(validate_prune_args(false, false, &["label=x".to_owned()]).is_err());
    }
}
