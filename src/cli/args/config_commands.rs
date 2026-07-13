//! cli args config commands module.
//!
//! Contains cli args config commands logic used by Stackctl command workflows.

use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum ConfigCommands {
    /// Migrate local .stackctl.toml or .stackctl.yaml to the latest supported schema
    Migrate,
    /// Print the bundled v8 project JSON Schema without loading project state
    Schema,
}
