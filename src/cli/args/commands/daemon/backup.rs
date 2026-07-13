use clap::Args;
use std::path::PathBuf;

/// Selects one exact logical data service for a verified recovery point.
#[derive(Args)]
pub(crate) struct DaemonBackupArgs {
    /// Declared service owning the logical data resource
    #[arg(value_name = "SERVICE")]
    pub(crate) service: String,
    /// Existing project directory registered by the singleton daemon
    #[arg(default_value = ".", value_name = "PATH")]
    pub(crate) path: PathBuf,
}
