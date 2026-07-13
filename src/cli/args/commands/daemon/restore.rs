use clap::Args;
use std::path::PathBuf;

/// Selects one exact verified recovery point for reversible restoration.
#[derive(Args)]
pub(crate) struct DaemonRestoreArgs {
    /// Immutable recovery-point ID shown by `stackctl daemon backups`
    #[arg(value_name = "RECOVERY_POINT_ID")]
    pub(crate) recovery_point_id: String,
    /// Existing project directory registered by the singleton daemon
    #[arg(default_value = ".", value_name = "PATH")]
    pub(crate) path: PathBuf,
}
