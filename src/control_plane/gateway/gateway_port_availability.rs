/// Current ownership observation for one required gateway socket.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum GatewayPortAvailability {
    Available,
    Occupied { owner: Option<String> },
}
