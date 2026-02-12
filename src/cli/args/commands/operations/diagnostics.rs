use clap::Args;
use std::path::PathBuf;

use crate::cli::args::EnvCommands;
use crate::config;

#[derive(Args)]
pub(crate) struct AboutArgs {}

#[derive(Args)]
pub(crate) struct StatusArgs {
    #[arg(long, default_value = "table")]
    pub(crate) format: String,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long, value_enum)]
    pub(crate) driver: Option<config::Driver>,
}

#[derive(Args)]
pub(crate) struct HealthArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Timeout in seconds
    #[arg(long, default_value_t = 30)]
    pub(crate) timeout: u64,
    /// Seconds between health checks
    #[arg(long, default_value_t = 2)]
    pub(crate) interval: u64,
    /// Maximum check attempts before failing
    #[arg(long)]
    pub(crate) retries: Option<u32>,
    #[arg(long, default_value_t = 1)]
    pub(crate) parallel: usize,
}

#[derive(Args)]
pub(crate) struct EnvArgs {
    #[command(subcommand)]
    pub(crate) command: Option<EnvCommands>,
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long)]
    pub(crate) env_file: Option<PathBuf>,
    /// Sync Helm-managed app env vars from running app container(s)
    #[arg(long, default_value_t = false)]
    pub(crate) sync: bool,
    /// Remove stale Helm-managed app env vars missing from sync/config
    #[arg(long, default_value_t = false)]
    pub(crate) purge: bool,
    /// Persist discovered runtime host/port bindings into `.helm.toml`
    #[arg(long, default_value_t = false, requires = "sync")]
    pub(crate) persist_runtime: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) create_missing: bool,
}

#[derive(Args)]
pub(crate) struct ListArgs {
    #[arg(long, default_value = "table")]
    pub(crate) format: String,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long, value_enum)]
    pub(crate) driver: Option<config::Driver>,
}
