use super::CompatibilityFingerprint;
use super::logical_service_consumer::LogicalServiceConsumer;

/// One validated project request for compatibility-keyed infrastructure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SharedServiceRequest {
    consumer: LogicalServiceConsumer,
    fingerprint: CompatibilityFingerprint,
}

impl SharedServiceRequest {
    pub(crate) fn new(
        project_id: impl Into<String>,
        service_id: impl Into<String>,
        fingerprint: CompatibilityFingerprint,
    ) -> Self {
        Self {
            consumer: LogicalServiceConsumer::new(project_id.into(), service_id.into()),
            fingerprint,
        }
    }

    pub(super) fn into_parts(self) -> (CompatibilityFingerprint, LogicalServiceConsumer) {
        (self.fingerprint, self.consumer)
    }
}
