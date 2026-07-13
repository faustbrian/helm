//! cli args config commands module.
//!
//! Contains cli args config commands logic used by Stackctl command workflows.

use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub(crate) enum ConfigCommands {
    /// Migrate local .stackctl.toml or .stackctl.yaml to the latest supported schema
    Migrate,
    /// Print the bundled v8 project JSON Schema without loading project state
    Schema,
    /// Validate and resolve one v8 project file without changing local state
    Validate {
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,
    },
}
