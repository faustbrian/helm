use super::WorkloadReconcileResult;
use crate::control_plane::engine::ImageId;
use crate::control_plane::gateway::GatewayRoute;

/// Runtime image, route, and workload state produced by one reconciliation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProjectRuntimeReconcileResult {
    runtime_image_id: ImageId,
    route: GatewayRoute,
    workload: WorkloadReconcileResult,
}

impl ProjectRuntimeReconcileResult {
    pub(super) const fn new(
        runtime_image_id: ImageId,
        route: GatewayRoute,
        workload: WorkloadReconcileResult,
    ) -> Self {
        Self {
            runtime_image_id,
            route,
            workload,
        }
    }

    pub(crate) const fn runtime_image_id(&self) -> &ImageId {
        &self.runtime_image_id
    }

    pub(crate) const fn route(&self) -> &GatewayRoute {
        &self.route
    }

    pub(crate) const fn workload(&self) -> &WorkloadReconcileResult {
        &self.workload
    }
}
