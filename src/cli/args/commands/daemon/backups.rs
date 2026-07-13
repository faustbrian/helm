use clap::Args;
use std::path::PathBuf;

/// Selects one exact project whose verified recovery points should be listed.
#[derive(Args)]
pub(crate) struct DaemonBackupsArgs {
    /// Existing project directory registered by the singleton daemon
    #[arg(default_value = ".", value_name = "PATH")]
    pub(crate) path: PathBuf,
}
