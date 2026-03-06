//! cli args commands app task module.
//!
//! Contains cli args for `helm task` workflows.

use clap::{Args, Subcommand};

use crate::cli::args::{PackageManagerArg, VersionManagerArg};
use crate::config;

#[derive(Args)]
pub(crate) struct TaskArgs {
    #[command(subcommand)]
    pub(crate) command: TaskCommands,
}

#[derive(Subcommand)]
pub(crate) enum TaskCommands {
    /// Run dependency-management workflows
    Deps(TaskDepsArgs),
}

#[derive(Args)]
pub(crate) struct TaskDepsArgs {
    #[command(subcommand)]
    pub(crate) command: TaskDepsCommands,
}

#[derive(Subcommand)]
pub(crate) enum TaskDepsCommands {
    /// Bump app dependency manifests and refresh locks
    Bump(TaskDepsBumpArgs),
}

#[derive(Args)]
pub(crate) struct TaskDepsBumpArgs {
    #[arg(long)]
    pub(crate) service: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) kind: Option<config::Kind>,
    /// Select a service profile (full, infra, data, app, web, api)
    #[arg(long, conflicts_with_all = ["service", "kind"])]
    pub(crate) profile: Option<String>,
    /// Run the Composer dependency bump workflow
    #[arg(
        long,
        default_value_t = false,
        conflicts_with = "all",
        required_unless_present_any = ["node", "bun", "deno", "all"]
    )]
    pub(crate) composer: bool,
    /// Run the Node dependency bump workflow
    #[arg(
        long,
        default_value_t = false,
        conflicts_with = "all",
        required_unless_present_any = ["composer", "bun", "deno", "all"]
    )]
    pub(crate) node: bool,
    /// Run the Bun dependency bump workflow
    #[arg(
        long,
        default_value_t = false,
        conflicts_with = "all",
        required_unless_present_any = ["composer", "node", "deno", "all"]
    )]
    pub(crate) bun: bool,
    /// Run the Deno dependency bump workflow
    #[arg(
        long,
        default_value_t = false,
        conflicts_with = "all",
        required_unless_present_any = ["composer", "node", "bun", "all"]
    )]
    pub(crate) deno: bool,
    /// Run all dependency bump workflows
    #[arg(
        long,
        default_value_t = false,
        conflicts_with_all = ["composer", "node", "bun", "deno"],
        required_unless_present_any = ["composer", "node", "bun", "deno"]
    )]
    pub(crate) all: bool,
    /// Override inferred Node package manager
    #[arg(long = "package-manager", value_enum)]
    pub(crate) package_manager: Option<PackageManagerArg>,
    /// Override the Node version manager used for Node workflows
    #[arg(long = "version-manager", value_enum)]
    pub(crate) version_manager: Option<VersionManagerArg>,
    /// Override the Node version used for Node workflows
    #[arg(long = "node-version")]
    pub(crate) node_version: Option<String>,
    #[arg(long, default_value_t = false, conflicts_with = "no_tty")]
    pub(crate) tty: bool,
    #[arg(long, default_value_t = false)]
    pub(crate) no_tty: bool,
}

impl TaskDepsBumpArgs {
    pub(crate) fn service(&self) -> Option<&str> {
        self.service.as_deref()
    }

    pub(crate) fn profile(&self) -> Option<&str> {
        self.profile.as_deref()
    }
}
