use clap::ValueEnum;
/// Supported Node package managers.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
}
