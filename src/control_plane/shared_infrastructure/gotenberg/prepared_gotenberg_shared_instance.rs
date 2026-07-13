use super::{GotenbergProjectResources, GotenbergSharedInstancePlan};
use crate::control_plane::shared_infrastructure::SharedServiceReconcileResult;
use crate::control_plane::state::{
    LogicalResourceRecord, LogicalResourceRecordOptions, ResourceLifecycle,
};

/// One stateless Gotenberg process and every project endpoint consumer.
pub(crate) struct PreparedGotenbergSharedInstance {
    instance: GotenbergSharedInstancePlan,
    projects: Vec<GotenbergProjectResources>,
}

impl PreparedGotenbergSharedInstance {
    pub(super) const fn new(
        instance: GotenbergSharedInstancePlan,
        projects: Vec<GotenbergProjectResources>,
    ) -> Self {
        Self { instance, projects }
    }

    pub(crate) const fn instance(&self) -> &GotenbergSharedInstancePlan {
        &self.instance
    }

    pub(crate) fn projects(&self) -> &[GotenbergProjectResources] {
        &self.projects
    }

    pub(crate) fn logical_record(
        &self,
        project: &GotenbergProjectResources,
        shared: &SharedServiceReconcileResult,
    ) -> LogicalResourceRecord {
        LogicalResourceRecord::new(LogicalResourceRecordOptions {
            logical_resource_id: format!(
                "{}/{}/gotenberg",
                project.project_id(),
                project.service_id()
            ),
            shared_resource_id: shared.container().id().as_str().to_owned(),
            project_id: project.project_id().to_owned(),
            service_id: project.service_id().to_owned(),
            kind: "gotenberg_endpoint".to_owned(),
            compatibility_fingerprint: self
                .instance
                .container()
                .metadata()
                .compatibility_fingerprint()
                .to_owned(),
            desired_revision: project.environment().revision().to_owned(),
            lifecycle: ResourceLifecycle::Active,
            orphaned_at_unix_seconds: None,
        })
    }
}
