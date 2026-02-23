//! cli args commands module.
//!
//! Contains cli args commands logic used by Helm command workflows.

use clap::Subcommand;

mod app;
mod lifecycle;
mod meta;
mod operations;

pub(crate) use app::{
    AppCreateArgs, ArtisanArgs, ComposerArgs, EnvScrubArgs, ExecArgs, NodeArgs, OpenArgs,
    ServeArgs, ShareArgs, ShareCommands, ShareProviderSelectionArgs,
};

#[cfg(test)]
pub(crate) use app::{ShareStartArgs, ShareStatusArgs, ShareStopArgs};

pub(crate) use lifecycle::{
    ApplyArgs, DownArgs, RecreateArgs, RelabelArgs, RestartArgs, RmArgs, SetupArgs, StartArgs,
    StopArgs, UpArgs, UpdateArgs, UrlArgs,
};

pub(crate) use meta::{CompletionsArgs, ConfigArgs, DoctorArgs, LockArgs, PresetArgs, ProfileArgs};

pub(crate) use operations::{
    AboutArgs, AttachArgs, CpArgs, DumpArgs, EnvArgs, EventsArgs, HealthArgs, InspectArgs,
    KillArgs, LogsArgs, LsArgs, PauseArgs, PortArgs, PruneArgs, PsArgs, PullArgs, RestoreArgs,
    StatsArgs, SwarmArgs, TopArgs, UnpauseArgs, WaitArgs,
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
    /// Run doctor, start services, bootstrap app runtime, then open app URLs
    Start(StartArgs),
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
    /// Recreate containers to apply current Helm ownership labels
    Relabel(RelabelArgs),
    /// Print connection URL(s)
    Url(UrlArgs),
    /// Restore a SQL file into a database service
    Restore(RestoreArgs),
    /// Dump a database service to a SQL file
    Dump(DumpArgs),
    /// List service runtime status
    #[command(visible_alias = "status")]
    Ps(PsArgs),
    /// Show project runtime overview
    About(AboutArgs),
    /// Check if service(s) are ready to accept connections
    Health(HealthArgs),
    /// Update .env with service connection values
    Env(EnvArgs),
    /// Show container logs
    Logs(LogsArgs),
    /// Show running processes in container(s)
    Top(TopArgs),
    /// Show a live stream of container resource usage
    Stats(StatsArgs),
    /// Show low-level details for container(s)
    Inspect(InspectArgs),
    /// Attach local standard input/output/error streams to a running container
    Attach(AttachArgs),
    /// Copy files/folders between host and container
    Cp(CpArgs),
    /// Force-stop running container(s)
    Kill(KillArgs),
    /// Pause all processes in container(s)
    Pause(PauseArgs),
    /// Unpause all processes in container(s)
    Unpause(UnpauseArgs),
    /// Block until container(s) stop and print exit status
    Wait(WaitArgs),
    /// Stream Docker daemon events (Helm container scope by default)
    Events(EventsArgs),
    /// List port mappings for container(s)
    Port(PortArgs),
    /// Remove stopped Helm service containers (or all with --all)
    Prune(PruneArgs),
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
    /// Share an app service through an external tunnel provider
    Share(ShareArgs),
    /// Scrub sensitive .env values with safe local placeholders
    EnvScrub(EnvScrubArgs),
}
