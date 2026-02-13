//! cli args commands app actions module.
//!
//! Contains cli args commands app actions logic used by Helm command workflows.

use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub(crate) struct AppCreateArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, default_value_t = false)]
    pub(crate) no_migrate: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) seed: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) no_storage_link: bool,
}

#[derive(Args)]
pub(crate) struct ServeArgs {
    /// App service name to serve
    #[arg(long)]
    pub(crate) service: Option<String>,
    /// Recreate the app container before starting serve
    #[arg(long, default_value_t = false)]
    pub(crate) recreate: bool,
    /// Run foreground serve process in detached container mode
    #[arg(long, default_value_t = false)]
    pub(crate) detached: bool,
    /// Write inferred service vars to local `.env`
    #[arg(long, default_value_t = false)]
    pub(crate) env_output: bool,
    /// Trust the inner serve container CA in system trust store
    #[arg(long, default_value_t = false)]
    pub(crate) trust_container_ca: bool,
}

#[derive(Args)]
pub(crate) struct OpenArgs {
    /// App service name to inspect/open
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, default_value_t = false, conflicts_with = "service")]
    pub(crate) all: bool,
    #[arg(long)]
    pub(crate) health_path: Option<String>,
    #[arg(long, default_value_t = false)]
    pub(crate) no_browser: bool,
    /// Print machine-readable JSON summary
    #[arg(long, default_value_t = false)]
    pub(crate) json: bool,
}

#[derive(Args)]
pub(crate) struct EnvScrubArgs {
    #[arg(long)]
    pub(crate) env_file: Option<PathBuf>,
}
