use super::{
    CertificateTrustStore, CurrentCaTrustResult, FilesystemCertificateStore, LocalCaIdentity,
    LocalCaTrustError, TrustChange, TrustStoreError, ensure_ca_trusted,
    reconcile_local_certificates,
};
use std::path::Path;
use time::OffsetDateTime;

/// Recovers or creates the singleton CA and installs its exact public identity.
pub(crate) fn install_current_ca_trust(
    certificates: &FilesystemCertificateStore,
    trust_store: &impl CertificateTrustStore,
    now: OffsetDateTime,
) -> Result<CurrentCaTrustResult, LocalCaTrustError> {
    let _rotation_lock = certificates.lock_trust_operation()?;
    let _lock = certificates.lock()?;
    let current = certificates.load_current()?;
    let reconciliation =
        reconcile_local_certificates(current.as_ref().map(|(bundle, _)| bundle), now)?;
    let paths = certificates.persist_inactive(reconciliation.bundle())?;
    let identity = LocalCaIdentity::from_pem(reconciliation.bundle().ca_certificate_pem())?;
    let change =
        install_and_activate_trust(trust_store, &identity, &paths.ca_certificate(), || {
            certificates.activate(&paths).map_err(Into::into)
        })?;

    Ok(CurrentCaTrustResult::new(identity, change))
}

pub(super) fn install_and_activate_trust(
    trust_store: &impl CertificateTrustStore,
    identity: &LocalCaIdentity,
    certificate_path: &Path,
    activate: impl FnOnce() -> Result<(), LocalCaTrustError>,
) -> Result<TrustChange, LocalCaTrustError> {
    let change = ensure_ca_trusted(trust_store, identity, certificate_path)?;
    let finalization = (|| {
        if !trust_store.contains(identity, certificate_path)? {
            return Err(
                TrustStoreError::new("Stackctl CA is not trusted after installation").into(),
            );
        }
        activate()
    })();
    if let Err(failure) = finalization {
        if change == TrustChange::Installed
            && let Err(rollback) = rollback_new_trust(trust_store, identity, certificate_path)
        {
            return Err(TrustStoreError::new(format!(
                "CA trust installation failed: {failure}; trust rollback failed: {rollback}"
            ))
            .into());
        }

        return Err(failure);
    }

    Ok(change)
}

fn rollback_new_trust(
    trust_store: &impl CertificateTrustStore,
    identity: &LocalCaIdentity,
    certificate_path: &Path,
) -> Result<(), TrustStoreError> {
    trust_store.remove(identity, certificate_path)?;
    if trust_store.contains(identity, certificate_path)? {
        return Err(TrustStoreError::new(
            "Stackctl CA remains trusted after rollback",
        ));
    }

    Ok(())
}
