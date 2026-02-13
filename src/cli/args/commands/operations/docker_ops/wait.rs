//! cli args commands operations docker ops wait module.
//!
//! Contains wait command args used by Helm command workflows.

use clap::Args;

use crate::cli::args::default_parallelism;
use crate::config;

#[derive(Args)]
pub(crate) struct WaitArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long)]
    pub(crate) condition: Option<String>,
    #[arg(long, default_value_t = default_parallelism())]
    pub(crate) parallel: usize,
}
