use super::logical_service_consumer::LogicalServiceConsumer;
use super::{
    CompatibilityFingerprint, CompatibilityProfile, SharedInstancePlan, SharedServiceRequest,
};
use std::collections::{BTreeMap, BTreeSet};

/// Groups complete project demand by exact immutable compatibility identity.
pub(crate) fn plan_shared_instances(
    requests: Vec<SharedServiceRequest>,
) -> Vec<SharedInstancePlan> {
    let mut consumers_by_fingerprint = BTreeMap::<
        CompatibilityFingerprint,
        (CompatibilityProfile, BTreeSet<LogicalServiceConsumer>),
    >::new();

    for request in requests {
        let (profile, consumer) = request.into_parts();
        consumers_by_fingerprint
            .entry(profile.fingerprint().clone())
            .or_insert_with(|| (profile, BTreeSet::new()))
            .1
            .insert(consumer);
    }

    consumers_by_fingerprint
        .into_iter()
        .map(|(_, (profile, consumers))| {
            SharedInstancePlan::new(profile, consumers.into_iter().collect())
        })
        .collect()
}
