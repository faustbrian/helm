use clap::Args;

use crate::config;

#[derive(Args)]
pub(crate) struct DownArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Also run `down` for workspace swarm dependencies after local down
    #[arg(long, default_value_t = false)]
    pub(crate) with_project_deps: bool,
    /// Allow downing shared workspace dependencies
    #[arg(long, default_value_t = false, requires = "with_project_deps")]
    pub(crate) force_project_dep_down: bool,
    #[arg(long, default_value_t = 1)]
    pub(crate) parallel: usize,
}

#[derive(Args)]
pub(crate) struct StopArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long, default_value_t = 1)]
    pub(crate) parallel: usize,
}

#[derive(Args)]
pub(crate) struct RmArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long, short, default_value_t = false)]
    pub(crate) force: bool,
    #[arg(long, default_value_t = 1)]
    pub(crate) parallel: usize,
}

#[derive(Args)]
pub(crate) struct RecreateArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Wait for service(s) to accept connections after recreating
    #[arg(long, default_value_t = false)]
    pub(crate) healthy: bool,
    /// Timeout in seconds for --healthy
    #[arg(long, default_value_t = 30)]
    pub(crate) timeout: u64,
    /// Assign random free host ports after start
    #[arg(long, default_value_t = false)]
    pub(crate) random_ports: bool,
    /// Persist random port assignments into `.helm.toml`
    #[arg(long, default_value_t = false, requires = "random_ports")]
    pub(crate) persist_ports: bool,
    /// Write inferred service vars to local `.env`
    #[arg(long, default_value_t = false)]
    pub(crate) write_env: bool,
    #[arg(long, default_value_t = 1)]
    pub(crate) parallel: usize,
}

#[derive(Args)]
pub(crate) struct RestartArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long, default_value_t = false)]
    pub(crate) healthy: bool,
    #[arg(long, default_value_t = 30)]
    pub(crate) timeout: u64,
    #[arg(long, default_value_t = 1)]
    pub(crate) parallel: usize,
}
