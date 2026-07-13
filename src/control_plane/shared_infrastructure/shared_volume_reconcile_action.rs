/// Mutation performed while converging one persistent shared volume.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SharedVolumeReconcileAction {
    Created,
    Unchanged,
}
