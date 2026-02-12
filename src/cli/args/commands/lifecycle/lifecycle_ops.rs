use clap::Args;

use crate::config;

#[derive(Args)]
pub(crate) struct DownArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Skip stopping workspace swarm dependencies
    #[arg(long, default_value_t = false)]
    pub(crate) no_deps: bool,
    /// Allow downing shared workspace dependencies
    #[arg(long, short, default_value_t = false, conflicts_with = "no_deps")]
    pub(crate) force: bool,
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
    pub(crate) wait: bool,
    /// Timeout in seconds for --wait
    #[arg(long, default_value_t = 30)]
    pub(crate) wait_timeout: u64,
    /// Publish all exposed ports to random host ports after start
    #[arg(long, short = 'P', default_value_t = false)]
    pub(crate) publish_all: bool,
    /// Persist random port assignments into `.helm.toml`
    #[arg(long, default_value_t = false, requires = "publish_all")]
    pub(crate) save_ports: bool,
    /// Write inferred service vars to local `.env`
    #[arg(long, default_value_t = false)]
    pub(crate) env_output: bool,
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
    pub(crate) wait: bool,
    #[arg(long, default_value_t = 30)]
    pub(crate) wait_timeout: u64,
    #[arg(long, default_value_t = 1)]
    pub(crate) parallel: usize,
}
