/// Container lifecycle or health transition emitted by the Engine.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum ContainerEventAction {
    Started,
    Stopped,
    Died,
    Destroyed,
    HealthStarting,
    HealthHealthy,
    HealthUnhealthy,
    Other(String),
}

impl ContainerEventAction {
    pub(super) fn from_engine_action(action: String) -> Self {
        match action.as_str() {
            "start" => Self::Started,
            "stop" => Self::Stopped,
            "die" => Self::Died,
            "destroy" => Self::Destroyed,
            "health_status: starting" => Self::HealthStarting,
            "health_status: healthy" => Self::HealthHealthy,
            "health_status: unhealthy" => Self::HealthUnhealthy,
            _ => Self::Other(action),
        }
    }
}
