//! cli args commands operations docker ops prune module.
//!
//! Contains prune command args used by Helm command workflows.

use clap::Args;

use crate::cli::args::default_parallelism;
use crate::config;

#[derive(Args)]
pub(crate) struct PruneArgs {
    #[arg(long, conflicts_with = "all")]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum, conflicts_with = "all")]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long, default_value_t = default_parallelism(), conflicts_with = "all")]
    pub(crate) parallel: usize,
    /// Prune all stopped Docker containers (not only Helm-managed services)
    #[arg(long, default_value_t = false, requires = "force")]
    pub(crate) all: bool,
    /// Skip confirmation prompt for global prune (`--all`)
    #[arg(long, short, default_value_t = false, requires = "all")]
    pub(crate) force: bool,
    /// Additional Docker prune filters (global mode only)
    #[arg(long, requires = "all")]
    pub(crate) filter: Vec<String>,
}
