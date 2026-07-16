use super::SharedVolumeReconcileAction;
use crate::control_plane::engine::OwnedVolume;

/// Proven owned volume and mutation resulting from one reconciliation pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SharedVolumeReconcileResult {
    volume: OwnedVolume,
    action: SharedVolumeReconcileAction,
}

impl SharedVolumeReconcileResult {
    pub(super) const fn new(volume: OwnedVolume, action: SharedVolumeReconcileAction) -> Self {
        Self { volume, action }
    }

    pub(crate) const fn volume(&self) -> &OwnedVolume {
        &self.volume
    }

    #[cfg(test)]
    pub(crate) const fn action(&self) -> SharedVolumeReconcileAction {
        self.action
    }
}
