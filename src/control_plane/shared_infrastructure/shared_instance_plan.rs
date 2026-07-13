use super::CompatibilityFingerprint;
use super::logical_service_consumer::LogicalServiceConsumer;

/// One physical instance and all logical project consumers it must provision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SharedInstancePlan {
    fingerprint: CompatibilityFingerprint,
    consumers: Vec<LogicalServiceConsumer>,
}

impl SharedInstancePlan {
    pub(super) fn new(
        fingerprint: CompatibilityFingerprint,
        consumers: Vec<LogicalServiceConsumer>,
    ) -> Self {
        Self {
            fingerprint,
            consumers,
        }
    }

    pub(crate) const fn fingerprint(&self) -> &CompatibilityFingerprint {
        &self.fingerprint
    }

    pub(crate) fn consumers(&self) -> &[LogicalServiceConsumer] {
        &self.consumers
    }
}
