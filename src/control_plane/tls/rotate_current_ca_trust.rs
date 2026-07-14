use super::{
    CertificateTrustStore, FilesystemCertificateStore, LocalCaIdentity, LocalCaRotationResult,
    LocalCaTrustError, LocalCertificateError, StoredCertificatePaths, TrustStoreError,
    ensure_ca_trusted, generate_local_certificates, remove_ca_trust,
};
use time::OffsetDateTime;

/// Replaces the singleton CA after its leaf is confirmed active at the gateway.
pub(crate) fn rotate_current_ca_trust<F>(
    certificates: &FilesystemCertificateStore,
    trust_store: &impl CertificateTrustStore,
    now: OffsetDateTime,
    after_activation: F,
) -> Result<LocalCaRotationResult, LocalCaTrustError>
where
    F: FnOnce(&LocalCaIdentity, &StoredCertificatePaths) -> Result<(), TrustStoreError>,
{
    let _rotation_lock = certificates.lock_rotation()?;
    let (previous_identity, previous_paths, current_identity, current_paths) = {
        let _lock = certificates.lock()?;
        let Some((previous_bundle, previous_paths)) = certificates.load_current()? else {
            return Err(LocalCertificateError::new(
                "Stackctl CA material does not exist; install trust before rotating it",
            )
            .into());
        };
        let previous_identity = LocalCaIdentity::from_pem(previous_bundle.ca_certificate_pem())?;
        if !trust_store
            .contains(&previous_identity, &previous_paths.ca_certificate())
            .map_err(|error| trust_error("verify previous Stackctl CA trust", error))?
        {
            return Err(trust_error(
                "verify previous Stackctl CA trust",
                "the active CA is not trusted",
            )
            .into());
        }

        let current_bundle = generate_local_certificates(now)?;
        let current_identity = LocalCaIdentity::from_pem(current_bundle.ca_certificate_pem())?;
        if current_identity == previous_identity {
            return Err(LocalCertificateError::new(
                "generated Stackctl CA unexpectedly matches the active identity",
            )
            .into());
        }
        let current_paths = certificates.persist_inactive(&current_bundle)?;

        if let Err(error) = install_and_verify(trust_store, &current_identity, &current_paths) {
            return Err(
                rollback_new_trust(trust_store, &current_identity, &current_paths, error).into(),
            );
        }
        if let Err(error) = certificates.activate(&current_paths) {
            return Err(rollback_to_previous(
                certificates,
                trust_store,
                &previous_identity,
                &previous_paths,
                &current_identity,
                &current_paths,
                trust_error("activate replacement Stackctl CA", error),
            ));
        }

        (
            previous_identity,
            previous_paths,
            current_identity,
            current_paths,
        )
    };

    if let Err(error) = after_activation(&current_identity, &current_paths) {
        let _lock = certificates.lock()?;
        return Err(rollback_to_previous(
            certificates,
            trust_store,
            &previous_identity,
            &previous_paths,
            &current_identity,
            &current_paths,
            error,
        ));
    }

    let _lock = certificates.lock()?;
    ensure_active_generation(certificates, &current_paths)?;
    if let Err(error) = remove_and_verify(trust_store, &previous_identity, &previous_paths) {
        return Err(rollback_to_previous(
            certificates,
            trust_store,
            &previous_identity,
            &previous_paths,
            &current_identity,
            &current_paths,
            error,
        ));
    }

    Ok(LocalCaRotationResult::new(
        previous_identity,
        current_identity,
    ))
}

fn install_and_verify(
    trust_store: &impl CertificateTrustStore,
    identity: &LocalCaIdentity,
    paths: &StoredCertificatePaths,
) -> Result<(), TrustStoreError> {
    ensure_ca_trusted(trust_store, identity, &paths.ca_certificate())
        .map_err(|error| trust_error("install replacement Stackctl CA trust", error))?;
    if !trust_store
        .contains(identity, &paths.ca_certificate())
        .map_err(|error| trust_error("verify replacement Stackctl CA trust", error))?
    {
        return Err(trust_error(
            "verify replacement Stackctl CA trust",
            "the replacement CA is not trusted after installation",
        ));
    }

    Ok(())
}

fn remove_and_verify(
    trust_store: &impl CertificateTrustStore,
    identity: &LocalCaIdentity,
    paths: &StoredCertificatePaths,
) -> Result<(), TrustStoreError> {
    remove_ca_trust(trust_store, identity, &paths.ca_certificate())
        .map_err(|error| trust_error("remove previous Stackctl CA trust", error))?;
    if trust_store
        .contains(identity, &paths.ca_certificate())
        .map_err(|error| trust_error("verify previous Stackctl CA removal", error))?
    {
        return Err(trust_error(
            "verify previous Stackctl CA removal",
            "the previous CA remains trusted after removal",
        ));
    }

    Ok(())
}

fn rollback_new_trust(
    trust_store: &impl CertificateTrustStore,
    identity: &LocalCaIdentity,
    paths: &StoredCertificatePaths,
    failure: TrustStoreError,
) -> TrustStoreError {
    match remove_ca_trust(trust_store, identity, &paths.ca_certificate()) {
        Ok(_) => failure,
        Err(rollback) => trust_error(
            "roll back replacement Stackctl CA trust",
            format!("{failure}; rollback also failed: {rollback}"),
        ),
    }
}

fn restore_previous_trust(
    trust_store: &impl CertificateTrustStore,
    previous_identity: &LocalCaIdentity,
    previous_paths: &StoredCertificatePaths,
    current_identity: &LocalCaIdentity,
    current_paths: &StoredCertificatePaths,
) -> Result<(), TrustStoreError> {
    install_and_verify(trust_store, previous_identity, previous_paths)?;
    remove_and_verify(trust_store, current_identity, current_paths)
}

fn rollback_to_previous(
    certificates: &FilesystemCertificateStore,
    trust_store: &impl CertificateTrustStore,
    previous_identity: &LocalCaIdentity,
    previous_paths: &StoredCertificatePaths,
    current_identity: &LocalCaIdentity,
    current_paths: &StoredCertificatePaths,
    failure: TrustStoreError,
) -> LocalCaTrustError {
    if let Err(pointer_rollback) = certificates.activate(previous_paths) {
        return trust_error(
            "roll back Stackctl CA rotation",
            format!(
                "{failure}; active-generation rollback also failed: {pointer_rollback}; \
                 both CA identities remain trusted"
            ),
        )
        .into();
    }

    match restore_previous_trust(
        trust_store,
        previous_identity,
        previous_paths,
        current_identity,
        current_paths,
    ) {
        Ok(()) => failure.into(),
        Err(trust_rollback) => trust_error(
            "roll back Stackctl CA rotation",
            format!("{failure}; trust rollback also failed: {trust_rollback}"),
        )
        .into(),
    }
}

fn ensure_active_generation(
    certificates: &FilesystemCertificateStore,
    expected: &StoredCertificatePaths,
) -> Result<(), LocalCaTrustError> {
    let Some((_bundle, active)) = certificates.load_current()? else {
        return Err(LocalCertificateError::new(
            "active Stackctl CA disappeared during gateway activation",
        )
        .into());
    };
    if active != *expected {
        return Err(LocalCertificateError::new(
            "active Stackctl CA changed during gateway activation",
        )
        .into());
    }

    Ok(())
}

fn trust_error(context: &str, error: impl std::fmt::Display) -> TrustStoreError {
    TrustStoreError::new(format!("{context}: {error}"))
}
