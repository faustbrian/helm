use clap::Args;

use crate::cli::args::PortStrategyArg;
use crate::config;

#[derive(Args)]
pub(crate) struct LogsArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long, default_value_t = false)]
    pub(crate) all: bool,
    /// Prefix each log line with service name
    #[arg(long, default_value_t = false)]
    pub(crate) prefix: bool,
    /// Follow log output
    #[arg(long, short, default_value_t = false)]
    pub(crate) follow: bool,
    #[arg(long)]
    pub(crate) tail: Option<u64>,
    #[arg(long, short = 't', default_value_t = false)]
    pub(crate) timestamps: bool,
    /// Tail local Caddy access logs instead of container logs
    #[arg(long, default_value_t = false)]
    pub(crate) access: bool,
}

#[derive(Args)]
pub(crate) struct SwarmArgs {
    /// Restrict to a comma-delimited subset of swarm target names
    #[arg(long, value_delimiter = ',')]
    pub(crate) only: Vec<String>,
    /// Run only selected targets and skip swarm dependencies
    #[arg(long, default_value_t = false)]
    pub(crate) no_deps: bool,
    /// Allow downing dependencies also required by non-selected targets
    #[arg(long, short, default_value_t = false, conflicts_with = "no_deps")]
    pub(crate) force: bool,
    /// Number of projects to process in parallel
    #[arg(long, default_value_t = 1)]
    pub(crate) parallel: usize,
    /// Stop after first failure (requires --parallel 1)
    #[arg(long, default_value_t = false)]
    pub(crate) fail_fast: bool,
    /// Strategy for assigning ports during `swarm up`
    #[arg(long, value_enum, default_value_t = PortStrategyArg::Random)]
    pub(crate) port_strategy: PortStrategyArg,
    /// Seed used by `--port-strategy stable`
    #[arg(long)]
    pub(crate) port_seed: Option<String>,
    /// Write inferred service vars to local `.env` during `swarm up`
    #[arg(long, default_value_t = false)]
    pub(crate) env_output: bool,
    /// Command to run, e.g. `up`, `down`, `ps --format json`
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command: Vec<String>,
}
