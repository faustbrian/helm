//! Strict v8 project-container command arguments.

use clap::Args;

use super::super::super::PackageManagerArg;

#[derive(Args)]
pub(crate) struct ExecArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command: Vec<String>,
}

impl ExecArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct ArtisanArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, default_value_t = false)]
    pub(crate) browser: bool,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command: Vec<String>,
}

impl ArtisanArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct ComposerArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command: Vec<String>,
}

impl ComposerArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct NodeArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long = "package-manager", value_enum)]
    pub(crate) package_manager: Option<PackageManagerArg>,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command: Vec<String>,
}

impl NodeArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct BunArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command: Vec<String>,
}

impl BunArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }
}

#[derive(Args)]
pub(crate) struct DenoArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) command: Vec<String>,
}

impl DenoArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }
}
