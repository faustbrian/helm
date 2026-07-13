/// The lifecycle state needed by reconciliation, independent of backend JSON.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum ContainerState {
    Running,
    Stopped,
    Missing,
}
