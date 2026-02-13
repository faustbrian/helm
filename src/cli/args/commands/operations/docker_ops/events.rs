//! cli args commands operations docker ops events module.
//!
//! Contains events command args used by Helm command workflows.

use clap::Args;

use crate::config;

#[derive(Args)]
pub(crate) struct EventsArgs {
    #[arg(long, conflicts_with = "all")]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum, conflicts_with = "all")]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long)]
    pub(crate) since: Option<String>,
    #[arg(long)]
    pub(crate) until: Option<String>,
    #[arg(long)]
    pub(crate) format: Option<String>,
    /// Stream all daemon events instead of filtering to Helm containers
    #[arg(long, default_value_t = false)]
    pub(crate) all: bool,
    #[arg(long)]
    pub(crate) filter: Vec<String>,
}
