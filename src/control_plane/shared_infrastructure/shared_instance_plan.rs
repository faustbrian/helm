use super::logical_service_consumer::LogicalServiceConsumer;
use super::{CompatibilityFingerprint, CompatibilityProfile};

/// One physical instance and all logical project consumers it must provision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SharedInstancePlan {
    profile: CompatibilityProfile,
    consumers: Vec<LogicalServiceConsumer>,
}

impl SharedInstancePlan {
    pub(super) fn new(
        profile: CompatibilityProfile,
        consumers: Vec<LogicalServiceConsumer>,
    ) -> Self {
        Self { profile, consumers }
    }

    pub(crate) const fn fingerprint(&self) -> &CompatibilityFingerprint {
        self.profile.fingerprint()
    }

    pub(crate) const fn profile(&self) -> &CompatibilityProfile {
        &self.profile
    }

    pub(crate) fn consumers(&self) -> &[LogicalServiceConsumer] {
        &self.consumers
    }
}
