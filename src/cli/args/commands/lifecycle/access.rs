use clap::Args;

use crate::config;

#[derive(Args)]
pub(crate) struct ConnectArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, default_value_t = false, conflicts_with = "no_tty")]
    pub(crate) tty: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) no_tty: bool,
}

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
