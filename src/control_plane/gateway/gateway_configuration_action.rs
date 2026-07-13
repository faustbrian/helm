/// Route configuration mutation performed by one reconciliation pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum GatewayConfigurationAction {
    Unchanged,
    Applied,
}
