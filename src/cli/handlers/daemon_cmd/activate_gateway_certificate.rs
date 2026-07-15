use crate::control_plane::{
    IpcOutcome, IpcPayload, IpcResult, StoredCertificatePaths, TrustStoreError,
    wait_for_gateway_certificate_generation,
};
use std::path::Path;
use std::time::Duration;

/// Requests reconciliation and waits for the ready gateway certificate marker.
pub(super) fn activate_gateway_certificate(
    runtime_directory: &Path,
    paths: &StoredCertificatePaths,
) -> Result<(), TrustStoreError> {
    let generation = paths
        .revision()
        .map_err(|error| TrustStoreError::new(error.to_string()))?;
    let response = super::send_singleton_request(IpcPayload::ActivateGatewayCertificate {
        generation: generation.to_owned(),
    })
    .map_err(|error| TrustStoreError::new(error.to_string()))?;
    match response.outcome() {
        IpcOutcome::Success {
            result:
                IpcResult::GatewayCertificateActivationRequested {
                    generation: accepted,
                },
        } if accepted == generation => {}
        IpcOutcome::Failure { diagnostics } => {
            let detail = diagnostics
                .iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.code(), diagnostic.message()))
                .collect::<Vec<_>>()
                .join("; ");
            return Err(TrustStoreError::new(format!(
                "gateway certificate activation failed: {detail}"
            )));
        }
        outcome => {
            return Err(TrustStoreError::new(format!(
                "unexpected gateway certificate activation response: {outcome:?}"
            )));
        }
    }

    wait_for_gateway_certificate_generation(
        runtime_directory,
        generation,
        Duration::from_secs(30),
        Duration::from_millis(50),
    )
    .map_err(|error| TrustStoreError::new(error.to_string()))
}
