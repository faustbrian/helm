//! Strict v8 PHP tool command arguments.

use clap::Args;

#[derive(Args)]
pub(crate) struct PhpToolArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command: Vec<String>,
}

impl PhpToolArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }
}
