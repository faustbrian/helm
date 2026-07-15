//! Strict v8 browser-opening arguments.

use clap::Args;

#[derive(Args)]
pub(crate) struct OpenArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, default_value_t = false, conflicts_with = "service")]
    pub(crate) all: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) no_browser: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) json: bool,
}

impl OpenArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct RunArgs {
    /// Exact named workflow declared in .stackctl.yaml
    pub(crate) workflow: String,
}
