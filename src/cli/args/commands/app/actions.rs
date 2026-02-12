use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub(crate) struct AppCreateArgs {
    #[arg(long)]
    pub(crate) target: Option<String>,
    #[arg(long, default_value_t = false)]
    pub(crate) no_migrate: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) seed: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) no_storage_link: bool,
}

#[derive(Args)]
pub(crate) struct ServeArgs {
    #[arg(long)]
    pub(crate) target: Option<String>,
    #[arg(long, default_value_t = false)]
    pub(crate) recreate: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) detached: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) write_env: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) trust_container_ca: bool,
}

#[derive(Args)]
pub(crate) struct OpenArgs {
    #[arg(long)]
    pub(crate) target: Option<String>,
    #[arg(long, default_value_t = false, conflicts_with = "target")]
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
