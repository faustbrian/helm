//! cli args commands lifecycle access module.
//!
//! Contains cli args commands lifecycle access logic used by Helm command workflows.

use clap::Args;

use crate::config;

#[derive(Args)]
pub(crate) struct UrlArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, default_value = "table")]
    pub(crate) format: String,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long, value_enum)]
    pub(crate) driver: Option<config::Driver>,
}
