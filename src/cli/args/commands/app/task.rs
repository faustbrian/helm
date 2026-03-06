//! cli args commands app task module.
//!
//! Contains cli args for `helm task` workflows.

use clap::{Args, Subcommand};

use crate::cli::args::PackageManagerArg;
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
        required_unless_present_any = ["node", "all"]
    )]
    pub(crate) composer: bool,
    /// Run the Node dependency bump workflow
    #[arg(
        long,
        default_value_t = false,
        conflicts_with = "all",
        required_unless_present_any = ["composer", "all"]
    )]
    pub(crate) node: bool,
    /// Run both Composer and Node dependency bump workflows
    #[arg(
        long,
        default_value_t = false,
        conflicts_with_all = ["composer", "node"],
        required_unless_present_any = ["composer", "node"]
    )]
    pub(crate) all: bool,
    /// Override inferred JS package manager
    #[arg(long, value_enum)]
    pub(crate) manager: Option<PackageManagerArg>,
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
