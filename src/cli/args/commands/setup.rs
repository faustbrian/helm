//! CLI arguments for one-time v8 control-plane setup.

use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub(crate) struct SetupArgs {
    /// Authoritative parent directory to scan for Stackctl projects
    #[arg(long, value_name = "DIR", required = true)]
    pub(crate) dir: Vec<PathBuf>,
    /// Seconds between correctness-fallback discovery scans
    #[arg(long, default_value_t = 30)]
    pub(crate) interval: u64,
}
