use clap::Subcommand;

mod app;
mod lifecycle;
mod meta;
mod operations;

use app::{
    AppCreateArgs, ArtisanArgs, ComposerArgs, EnvScrubArgs, ExecArgs, NodeArgs, OpenArgs, ServeArgs,
};
use lifecycle::{
    ApplyArgs, DownArgs, RecreateArgs, RestartArgs, RmArgs, SetupArgs, StopArgs, UpArgs,
    UpdateArgs, UrlArgs,
};
use meta::{CompletionsArgs, ConfigArgs, DoctorArgs, LockArgs, PresetArgs, ProfileArgs};
use operations::{
    AboutArgs, DumpArgs, EnvArgs, HealthArgs, LogsArgs, LsArgs, PsArgs, PullArgs, RestoreArgs,
    SwarmArgs,
};

#[derive(Subcommand)]
#[non_exhaustive]
pub(crate) enum Commands {
    /// Initialize a new .helm.toml config file
    Init,
    /// Print resolved configuration
    Config(ConfigArgs),
    /// Inspect available service presets
    Preset(PresetArgs),
    /// Inspect profile groupings
    Profile(ProfileArgs),
    /// Validate local setup and config health
    Doctor(DoctorArgs),
    /// Manage workspace lockfile for reproducible image resolution
    Lock(LockArgs),
    /// Prepare service(s)
    Setup(SetupArgs),
    /// Start service container(s)
    Up(UpArgs),
    /// Converge services and apply configured data seeds
    Apply(ApplyArgs),
    /// Pull latest images and restart selected services
    Update(UpdateArgs),
    /// Stop and remove service container(s)
    Down(DownArgs),
    /// Stop service container(s)
    Stop(StopArgs),
    /// Remove service container(s)
    Rm(RmArgs),
    /// Destroy and recreate service container(s) from scratch
    Recreate(RecreateArgs),
    /// Restart service container(s)
    Restart(RestartArgs),
    /// Print connection URL(s)
    Url(UrlArgs),
    /// Restore a SQL file into a database service
    Restore(RestoreArgs),
    /// Dump a database service to a SQL file
    Dump(DumpArgs),
    /// List service runtime status
    Ps(PsArgs),
    /// Show project runtime overview
    About(AboutArgs),
    /// Check if service(s) are ready to accept connections
    Health(HealthArgs),
    /// Update .env with service connection values
    Env(EnvArgs),
    /// Show container logs
    Logs(LogsArgs),
    /// Pull latest service image(s)
    Pull(PullArgs),
    /// Run a command inside a service container
    Exec(ExecArgs),
    /// Bootstrap Laravel app runtime (key, cache clear, migrate, storage link)
    AppCreate(AppCreateArgs),
    /// Run php artisan inside a serve container
    Artisan(ArtisanArgs),
    /// Run composer inside an app container
    Composer(ComposerArgs),
    /// Run JS package manager command inside an app container
    Node(NodeArgs),
    /// List configured services
    Ls(LsArgs),
    /// Run a helm command across configured swarm targets
    Swarm(SwarmArgs),
    /// Generate shell completions
    Completions(CompletionsArgs),
    /// Start an app serve target and expose it via local HTTPS domain
    Serve(ServeArgs),
    /// Print/open serve URL and show app/database health summary
    Open(OpenArgs),
    /// Scrub sensitive .env values with safe local placeholders
    EnvScrub(EnvScrubArgs),
}
