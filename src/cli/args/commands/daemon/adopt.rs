use clap::Args;
use std::path::PathBuf;

/// Selects the exact registered project path to adopt.
#[derive(Args)]
pub(crate) struct DaemonAdoptArgs {
    /// Existing project directory whose retained state should be reactivated
    #[arg(default_value = ".", value_name = "PATH")]
    pub(crate) path: PathBuf,
}
