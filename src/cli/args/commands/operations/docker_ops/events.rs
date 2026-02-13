//! cli args commands operations docker ops events module.
//!
//! Contains events command args used by Helm command workflows.

use clap::Args;

use crate::config;

#[derive(Args)]
#[command(
    after_help = "Examples:\n  helm events\n  helm events --service db\n  helm events --all --filter type=container"
)]
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
    /// Docker template passed to `docker events --format`
    pub(crate) format: Option<String>,
    /// Emit newline-delimited JSON objects (`--format {{json .}}`)
    #[arg(long, default_value_t = false, conflicts_with = "format")]
    pub(crate) json: bool,
    /// Stream all daemon events instead of filtering to Helm containers
    #[arg(long, default_value_t = false)]
    pub(crate) all: bool,
    /// Return success when no services match current filters
    #[arg(long, default_value_t = false, conflicts_with = "all")]
    pub(crate) allow_empty: bool,
    #[arg(long)]
    pub(crate) filter: Vec<String>,
}
