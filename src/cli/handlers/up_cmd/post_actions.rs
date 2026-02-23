//! Post-start actions for `up` command flows.

use anyhow::Result;
use std::path::Path;

use crate::{cli, config};

use super::apply_data_seeds;

pub(super) struct PostUpActionsOptions<'a> {
    pub(super) service: Option<&'a str>,
    pub(super) kind: Option<config::Kind>,
    pub(super) profile: Option<&'a str>,
    pub(super) seed: bool,
    pub(super) workspace_root: &'a Path,
    pub(super) quiet: bool,
}

pub(super) fn run_post_up_actions(
    config: &mut config::Config,
    options: PostUpActionsOptions<'_>,
) -> Result<()> {
    if options.seed {
        apply_data_seeds(
            config,
            options.service,
            options.kind,
            options.profile,
            options.workspace_root,
            options.quiet,
        )?;
    }

    cli::hooks::run_hooks_for_up_selection(
        config,
        options.service,
        options.kind,
        options.profile,
        config::HookPhase::PostUp,
        options.workspace_root,
        options.quiet,
    )?;
    Ok(())
}
