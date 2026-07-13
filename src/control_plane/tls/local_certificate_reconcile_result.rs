use super::{LocalCertificateBundle, LocalCertificateReconcileAction};

/// Complete certificate material and decision produced by one lifecycle pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LocalCertificateReconcileResult {
    bundle: LocalCertificateBundle,
    action: LocalCertificateReconcileAction,
}

impl LocalCertificateReconcileResult {
    pub(super) const fn new(
        bundle: LocalCertificateBundle,
        action: LocalCertificateReconcileAction,
    ) -> Self {
        Self { bundle, action }
    }

    pub(crate) const fn bundle(&self) -> &LocalCertificateBundle {
        &self.bundle
    }

    pub(crate) const fn action(&self) -> LocalCertificateReconcileAction {
        self.action
    }
}
