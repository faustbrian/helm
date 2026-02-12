use clap::Args;

use super::super::super::PackageManagerArg;

#[derive(Args)]
pub(crate) struct ExecArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, default_value_t = false, conflicts_with = "no_tty")]
    pub(crate) tty: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) no_tty: bool,
    /// Command and arguments to run
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command: Vec<String>,
}

#[derive(Args)]
pub(crate) struct ArtisanArgs {
    #[arg(long)]
    pub(crate) target: Option<String>,
    #[arg(long, default_value_t = false, conflicts_with = "no_tty")]
    pub(crate) tty: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) no_tty: bool,
    /// Artisan command and arguments
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command: Vec<String>,
}

#[derive(Args)]
pub(crate) struct ComposerArgs {
    #[arg(long)]
    pub(crate) target: Option<String>,
    #[arg(long, default_value_t = false, conflicts_with = "no_tty")]
    pub(crate) tty: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) no_tty: bool,
    /// Composer command and arguments
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command: Vec<String>,
}

#[derive(Args)]
pub(crate) struct NodeArgs {
    #[arg(long)]
    pub(crate) target: Option<String>,
    #[arg(long, value_enum, default_value_t = PackageManagerArg::Bun)]
    pub(crate) manager: PackageManagerArg,
    #[arg(long, default_value_t = false, conflicts_with = "no_tty")]
    pub(crate) tty: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) no_tty: bool,
    /// Package manager command and arguments
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command: Vec<String>,
}
