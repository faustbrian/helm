//! cli args commands operations docker ops stats module.
//!
//! Contains stats command args used by Helm command workflows.

use clap::Args;

use crate::config;

#[derive(Args)]
pub(crate) struct StatsArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Disable streaming and show a single snapshot
    #[arg(long, default_value_t = false)]
    pub(crate) no_stream: bool,
    #[arg(long)]
    pub(crate) format: Option<String>,
}
