/// Mutation performed by one gateway reconciliation pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum GatewayReconcileAction {
    Unchanged,
    Created,
    Started,
    Restarted,
    Replaced,
}
