/// Mutation performed while converging one retained project volume.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProjectVolumeReconcileAction {
    Created,
    Unchanged,
}
