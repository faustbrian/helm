use super::logical_service_consumer::LogicalServiceConsumer;
use super::{CompatibilityFingerprint, SharedInstancePlan, SharedServiceRequest};
use std::collections::{BTreeMap, BTreeSet};

/// Groups complete project demand by exact immutable compatibility identity.
pub(crate) fn plan_shared_instances(
    requests: Vec<SharedServiceRequest>,
) -> Vec<SharedInstancePlan> {
    let mut consumers_by_fingerprint =
        BTreeMap::<CompatibilityFingerprint, BTreeSet<LogicalServiceConsumer>>::new();

    for request in requests {
        let (fingerprint, consumer) = request.into_parts();
        consumers_by_fingerprint
            .entry(fingerprint)
            .or_default()
            .insert(consumer);
    }

    consumers_by_fingerprint
        .into_iter()
        .map(|(fingerprint, consumers)| {
            SharedInstancePlan::new(fingerprint, consumers.into_iter().collect())
        })
        .collect()
}
