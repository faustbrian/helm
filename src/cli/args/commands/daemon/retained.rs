use clap::Args;

/// Selects the output format for daemon-authoritative retained project state.
#[derive(Args)]
pub(crate) struct DaemonRetainedArgs {
    #[arg(long, default_value = "table")]
    pub(crate) format: String,
}
