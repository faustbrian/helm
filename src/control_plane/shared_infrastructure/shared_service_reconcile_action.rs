/// Mutation performed while converging one compatibility-keyed service.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SharedServiceReconcileAction {
    Created,
    Unchanged,
    Started,
    Restarted,
    Replaced,
}
