use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum ConfigCommands {
    /// Migrate local .helm.toml to the latest supported schema
    Migrate,
}
