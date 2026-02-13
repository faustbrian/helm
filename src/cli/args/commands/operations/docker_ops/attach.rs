//! cli args commands operations docker ops attach module.
//!
//! Contains attach command args used by Helm command workflows.

use clap::Args;

#[derive(Args)]
pub(crate) struct AttachArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, default_value_t = false)]
    pub(crate) no_stdin: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) sig_proxy: bool,
    #[arg(long)]
    pub(crate) detach_keys: Option<String>,
}
