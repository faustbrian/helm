use serde::{Deserialize, Serialize};

/// Last Engine-observed process and healthcheck state for one exact resource.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub(crate) enum IpcResourceHealth {
    EngineUnavailable,
    Unknown,
    Missing,
    Stopped,
    Restarting,
    RunningUnverified,
    Starting,
    Healthy,
    Unhealthy { failing_streak: u64 },
    ServiceNotReady { attempt: u32 },
}

impl IpcResourceHealth {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::EngineUnavailable => "engine_unavailable",
            Self::Unknown => "unknown",
            Self::Missing => "missing",
            Self::Stopped => "stopped",
            Self::Restarting => "restarting",
            Self::RunningUnverified => "running_unverified",
            Self::Starting => "starting",
            Self::Healthy => "healthy",
            Self::Unhealthy { .. } => "unhealthy",
            Self::ServiceNotReady { .. } => "service_not_ready",
        }
    }
}
