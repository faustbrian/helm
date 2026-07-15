//! Strict v8 CLI command surface.

use clap::Subcommand;

mod app;
mod daemon;
mod lifecycle;
mod meta;
mod operations;
mod setup;

pub(crate) use app::{
    ArtisanArgs, BunArgs, ComposerArgs, DenoArgs, ExecArgs, NodeArgs, OpenArgs, PhpToolArgs,
    RunArgs,
};
pub(crate) use daemon::{
    BenchmarkEvidenceScenario, DaemonAdoptArgs, DaemonArgs, DaemonBackupArgs, DaemonBackupsArgs,
    DaemonBenchmarkArgs, DaemonCommands, DaemonMigrationArgs, DaemonMigrationCommands,
    DaemonMigrationDecisionArgs, DaemonMigrationStatusArgs, DaemonPruneArgs, DaemonPruneCommands,
    DaemonPruneExecuteArgs, DaemonPrunePlanArgs, DaemonRestoreArgs, DaemonRetainedArgs,
    DaemonServiceArgs, DaemonServiceCommands, DaemonServiceInstallArgs, DaemonServicePrintArgs,
    DaemonServiceUninstallArgs, DaemonTrustArgs, DaemonTrustCommands, DaemonWatchArgs,
};
pub(crate) use lifecycle::UrlArgs;
pub(crate) use meta::{CompletionsArgs, ConfigArgs, LockArgs};
pub(crate) use operations::{EnvArgs, LogsArgs, PsArgs};
pub(crate) use setup::SetupArgs;

#[derive(Subcommand)]
#[non_exhaustive]
pub(crate) enum Commands {
    /// Verify and install the per-user v8 control plane
    Setup(SetupArgs),
    /// Inspect or validate strict v8 YAML configuration
    Config(ConfigArgs),
    /// Manage the immutable v8 artifact lock
    Lock(LockArgs),
    /// Manage the authoritative per-user Stackctl daemon
    Daemon(DaemonArgs),
    /// Print authoritative project route(s)
    Url(UrlArgs),
    /// Show authoritative project runtime status
    #[command(visible_alias = "status")]
    Ps(PsArgs),
    /// Export daemon-owned managed environment values
    Env(EnvArgs),
    /// Stream project logs through the daemon
    Logs(LogsArgs),
    /// Run a non-interactive command inside a project container
    Exec(ExecArgs),
    /// Run PHP Artisan inside a project container
    Artisan(ArtisanArgs),
    /// Run Composer inside a project container
    Composer(ComposerArgs),
    /// Run PHPStan inside a project container
    Phpstan(PhpToolArgs),
    /// Run ECS inside a project container
    Ecs(PhpToolArgs),
    /// Run PHP CS Fixer inside a project container
    PhpCsFixer(PhpToolArgs),
    /// Run Psalm inside a project container
    Psalm(PhpToolArgs),
    /// Run Pint inside a project container
    Pint(PhpToolArgs),
    /// Run Pest inside a project container
    Pest(PhpToolArgs),
    /// Run PHPUnit inside a project container
    Phpunit(PhpToolArgs),
    /// Run Rector inside a project container
    Rector(PhpToolArgs),
    /// Run a Node package-manager command inside a project container
    Node(NodeArgs),
    /// Run Bun inside a project container
    Bun(BunArgs),
    /// Run Deno inside a project container
    Deno(DenoArgs),
    /// Generate shell completions
    Completions(CompletionsArgs),
    /// Open one or all authoritative project routes
    Open(OpenArgs),
    /// Run one explicitly declared project workflow
    Run(RunArgs),
}
