/// Process and Engine-healthcheck state used by reconciliation diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum ContainerHealth {
    Missing,
    Stopped,
    Restarting,
    RunningUnverified,
    Starting,
    Healthy,
    Unhealthy { failing_streak: u64 },
}
