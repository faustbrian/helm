use clap::ValueEnum;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum PullPolicyArg {
    Always,
    Missing,
    Never,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub(crate) enum PackageManagerArg {
    Bun,
    Npm,
    Pnpm,
    Yarn,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum PortStrategyArg {
    Random,
    Stable,
}
