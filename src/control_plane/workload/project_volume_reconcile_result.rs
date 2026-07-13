use super::ProjectVolumeReconcileAction;
use crate::control_plane::engine::OwnedVolume;

/// Proven retained project volume and its reconciliation action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectVolumeReconcileResult {
    volume: OwnedVolume,
    action: ProjectVolumeReconcileAction,
}

impl ProjectVolumeReconcileResult {
    pub(super) const fn new(volume: OwnedVolume, action: ProjectVolumeReconcileAction) -> Self {
        Self { volume, action }
    }

    pub(crate) const fn volume(&self) -> &OwnedVolume {
        &self.volume
    }

    pub(crate) const fn action(&self) -> ProjectVolumeReconcileAction {
        self.action
    }
}
