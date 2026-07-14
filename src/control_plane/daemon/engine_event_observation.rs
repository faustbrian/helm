/// Bounded result of polling the persistent managed-container event stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum EngineEventObservation {
    Idle,
    Events { count: usize },
    Disconnected { detail: String },
}
