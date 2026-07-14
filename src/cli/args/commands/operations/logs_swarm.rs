//! Strict v8 project log arguments.

use clap::Args;

#[derive(Args)]
pub(crate) struct LogsArgs {
    #[arg(long, conflicts_with = "all")]
    pub(crate) service: Vec<String>,
    #[arg(long, default_value_t = false, conflicts_with = "service")]
    pub(crate) all: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) prefix: bool,
    #[arg(long, short, default_value_t = false)]
    pub(crate) follow: bool,
    #[arg(long)]
    pub(crate) tail: Option<u64>,
}

impl LogsArgs {
    pub(crate) fn services(&self) -> &[String] {
        &self.service
    }
}
