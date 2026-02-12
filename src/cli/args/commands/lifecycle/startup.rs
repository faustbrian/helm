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
    /// Wait for service(s) to accept connections
    #[arg(long, default_value_t = false)]
    pub(crate) healthy: bool,
    /// Timeout in seconds for --healthy
    #[arg(long, default_value_t = 30)]
    pub(crate) timeout: u64,
    /// Pull image before start
    #[arg(long, value_enum, default_value_t = PullPolicyArg::Missing)]
    pub(crate) pull: PullPolicyArg,
    /// Force container recreation before start
    #[arg(long, default_value_t = false)]
    pub(crate) recreate: bool,
    /// Assign random free host ports after start (enabled by default for `up`)
    #[arg(long, default_value_t = false)]
    pub(crate) random_ports: bool,
    /// Force random ports even when explicit `port` is configured
    #[arg(long, default_value_t = false)]
    pub(crate) force_random_ports: bool,
    /// Strategy for assigning ports when randomization is enabled
    #[arg(long, value_enum, default_value_t = PortStrategyArg::Random)]
    pub(crate) port_strategy: PortStrategyArg,
    /// Seed used by `--port-strategy stable`
    #[arg(long)]
    pub(crate) port_seed: Option<String>,
    /// Persist random port assignments into `.helm.toml`
    #[arg(long, default_value_t = false, requires = "random_ports")]
    pub(crate) persist_ports: bool,
    /// Write inferred service vars to local `.env`
    #[arg(long, default_value_t = false)]
    pub(crate) write_env: bool,
    /// Also run `up` for workspace swarm dependencies first
    #[arg(long, default_value_t = false)]
    pub(crate) with_project_deps: bool,
    /// Apply configured data seed files after startup
    #[arg(long, default_value_t = false)]
    pub(crate) with_data: bool,
    #[arg(long, default_value_t = 1)]
    pub(crate) parallel: usize,
}

#[derive(Args)]
pub(crate) struct ApplyArgs {
    /// Also run `up` for workspace swarm dependencies first
    #[arg(long, default_value_t = false)]
    pub(crate) with_project_deps: bool,
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
    pub(crate) recreate: bool,
    /// Skip building/rebuilding derived app image layers
    #[arg(long, default_value_t = false)]
    pub(crate) no_rebuild_app_image: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) healthy: bool,
    #[arg(long, default_value_t = 30)]
    pub(crate) timeout: u64,
}
