//! cli args commands operations docker ops top module.
//!
//! Contains top command args used by Helm command workflows.

use clap::Args;

use crate::config;

#[derive(Args)]
pub(crate) struct TopArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Optional ps args passed to `docker top`
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) args: Vec<String>,
}

impl TopArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) const fn kind(&self) -> Option<config::Kind> {
        self.kind
    }
}
