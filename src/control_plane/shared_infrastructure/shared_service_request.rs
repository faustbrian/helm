use super::CompatibilityProfile;
use super::logical_service_consumer::LogicalServiceConsumer;

/// One validated project request for compatibility-keyed infrastructure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SharedServiceRequest {
    consumer: LogicalServiceConsumer,
    profile: CompatibilityProfile,
}

impl SharedServiceRequest {
    pub(crate) fn new(
        project_id: impl Into<String>,
        service_id: impl Into<String>,
        profile: CompatibilityProfile,
    ) -> Self {
        Self {
            consumer: LogicalServiceConsumer::new(project_id.into(), service_id.into()),
            profile,
        }
    }

    pub(super) fn into_parts(self) -> (CompatibilityProfile, LogicalServiceConsumer) {
        (self.profile, self.consumer)
    }
}
