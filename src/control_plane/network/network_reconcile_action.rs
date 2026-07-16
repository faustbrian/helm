/// Mutation performed while converging one exact managed private network.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NetworkReconcileAction {
    Created,
    Unchanged,
}
