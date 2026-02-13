//! cli args commands operations docker ops inspect module.
//!
//! Contains inspect command args used by Helm command workflows.

use clap::Args;

use crate::config;

#[derive(Args)]
pub(crate) struct InspectArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long)]
    pub(crate) format: Option<String>,
    #[arg(long, default_value_t = false)]
    pub(crate) size: bool,
    #[arg(long = "type")]
    pub(crate) object_type: Option<String>,
}
