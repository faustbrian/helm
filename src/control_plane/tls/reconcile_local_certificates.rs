use super::{
    LocalCertificateBundle, LocalCertificateError, LocalCertificateReconcileAction,
    LocalCertificateReconcileResult, generate_local_certificates, renew_local_leaf_certificate,
};
use time::OffsetDateTime;

/// Generates or renews local TLS material according to its persisted deadline.
pub(crate) fn reconcile_local_certificates(
    current: Option<&LocalCertificateBundle>,
    now: OffsetDateTime,
) -> Result<LocalCertificateReconcileResult, LocalCertificateError> {
    let (bundle, action) = match current {
        None => (
            generate_local_certificates(now)?,
            LocalCertificateReconcileAction::Generated,
        ),
        Some(bundle) if now >= bundle.leaf_renew_after() => (
            renew_local_leaf_certificate(bundle, now)?,
            LocalCertificateReconcileAction::Renewed,
        ),
        Some(bundle) => (bundle.clone(), LocalCertificateReconcileAction::Unchanged),
    };

    Ok(LocalCertificateReconcileResult::new(bundle, action))
}
