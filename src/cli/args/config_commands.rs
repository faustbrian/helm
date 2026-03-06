//! cli args config commands module.
//!
//! Contains cli args config commands logic used by Helm command workflows.

use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum ConfigCommands {
    /// Migrate local .helm.toml to the latest supported schema
    Migrate,
}
