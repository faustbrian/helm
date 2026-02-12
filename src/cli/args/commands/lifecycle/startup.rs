use clap::Args;

use crate::config;

use super::super::super::{PortStrategyArg, PullPolicyArg};

#[derive(Args)]
pub(crate) struct SetupArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    #[arg(long, default_value_t = 30)]
    pub(crate) timeout: u64,
    #[arg(long, default_value_t = 1)]
    pub(crate) parallel: usize,
}

#[derive(Args)]
pub(crate) struct UpArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Start a named service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    /// Wait for service(s) to accept connections (enabled by default for `up`)
    #[arg(long, default_value_t = false)]
    pub(crate) wait: bool,
    /// Timeout in seconds for --wait
    #[arg(long, default_value_t = 30)]
    pub(crate) wait_timeout: u64,
    /// Pull image before start
    #[arg(long, value_enum, default_value_t = PullPolicyArg::Missing)]
    pub(crate) pull: PullPolicyArg,
    /// Recreate containers even if their configuration and image haven't changed
    #[arg(long, default_value_t = false)]
    pub(crate) force_recreate: bool,
    /// Publish all exposed ports to random host ports (enabled by default for `up`)
    #[arg(long, short = 'P', default_value_t = false)]
    pub(crate) publish_all: bool,
    /// Strategy for assigning ports when randomization is enabled
    #[arg(long, value_enum, default_value_t = PortStrategyArg::Random)]
    pub(crate) port_strategy: PortStrategyArg,
    /// Seed used by `--port-strategy stable`
    #[arg(long)]
    pub(crate) port_seed: Option<String>,
    /// Persist random port assignments into `.helm.toml`
    #[arg(long, default_value_t = false, requires = "publish_all")]
    pub(crate) save_ports: bool,
    /// Write inferred service vars to local `.env`
    #[arg(long, default_value_t = false)]
    pub(crate) env_output: bool,
    /// Skip starting workspace swarm dependencies
    #[arg(long, default_value_t = false)]
    pub(crate) no_deps: bool,
    /// Apply configured data seed files after startup
    #[arg(long, default_value_t = false)]
    pub(crate) seed: bool,
    #[arg(long, default_value_t = 1)]
    pub(crate) parallel: usize,
}

#[derive(Args)]
pub(crate) struct ApplyArgs {
    /// Skip starting workspace swarm dependencies
    #[arg(long, default_value_t = false)]
    pub(crate) no_deps: bool,
}

#[derive(Args)]
pub(crate) struct UpdateArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Update a named service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    #[arg(long, default_value_t = false)]
    pub(crate) force_recreate: bool,
    /// Do not build images before starting containers
    #[arg(long, default_value_t = false)]
    pub(crate) no_build: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) wait: bool,
    #[arg(long, default_value_t = 30)]
    pub(crate) wait_timeout: u64,
}
