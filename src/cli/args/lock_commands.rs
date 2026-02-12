use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum LockCommands {
    /// Resolve configured service images to immutable digests
    Images,
    /// Verify lockfile is present and in sync with current config
    Verify,
    /// Show the lockfile changes that would be written
    Diff,
}
