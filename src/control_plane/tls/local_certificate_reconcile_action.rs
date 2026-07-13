/// Certificate material mutation performed by one lifecycle pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum LocalCertificateReconcileAction {
    Unchanged,
    Generated,
    Renewed,
}
