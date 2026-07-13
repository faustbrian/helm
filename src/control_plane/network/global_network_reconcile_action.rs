/// Mutation performed while converging the installation-wide private network.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum GlobalNetworkReconcileAction {
    Created,
    Unchanged,
}
