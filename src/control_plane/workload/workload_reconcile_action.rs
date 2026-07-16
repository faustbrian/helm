/// Container mutation performed by one project workload reconciliation pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum WorkloadReconcileAction {
    Unchanged,
    Created,
    Started,
    Replaced,
}
