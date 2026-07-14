//! Strict v8 route lookup arguments.

use clap::Args;

#[derive(Args)]
pub(crate) struct UrlArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, default_value = "table")]
    pub(crate) format: String,
}

impl UrlArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }
}
