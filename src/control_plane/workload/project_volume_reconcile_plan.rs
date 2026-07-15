use crate::control_plane::engine::{OwnedVolume, VolumeCreateOptions};

/// Mutation-free decision for one retained project volume.
pub(crate) enum ProjectVolumeReconcilePlan {
    Create(VolumeCreateOptions),
    Adopt(OwnedVolume),
}
