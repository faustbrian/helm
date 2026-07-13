use clap::{Args, Subcommand};

#[derive(Args)]
pub(crate) struct DaemonPruneArgs {
    #[command(subcommand)]
    pub(crate) command: DaemonPruneCommands,
}

#[derive(Subcommand)]
pub(crate) enum DaemonPruneCommands {
    /// Plan one exact retained PostgreSQL tenant deletion without mutation
    Plan(DaemonPrunePlanArgs),
}

/// Selects exact retained state and verified recovery evidence for deletion.
#[derive(Args)]
pub(crate) struct DaemonPrunePlanArgs {
    /// Durable project identity retained after the project was unregistered
    #[arg(value_name = "PROJECT_ID")]
    pub(crate) project_id: String,
    /// Exact retained PostgreSQL service identity
    #[arg(value_name = "SERVICE_ID")]
    pub(crate) service_id: String,
    /// Exact verified recovery point authorizing the deletion plan
    #[arg(value_name = "RECOVERY_POINT_ID")]
    pub(crate) recovery_point_id: String,
}
