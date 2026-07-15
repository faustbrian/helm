use super::{MySqlProjectResources, MySqlSharedInstancePlan};
use crate::control_plane::shared_infrastructure::SharedServiceReconcileResult;
use crate::control_plane::state::{
    LogicalResourceRecord, LogicalResourceRecordOptions, ResourceLifecycle,
};

/// One physical MySQL-family plan and every isolated project tenant it owns.
pub(crate) struct PreparedMySqlSharedInstance {
    instance: MySqlSharedInstancePlan,
    projects: Vec<MySqlProjectResources>,
}

impl PreparedMySqlSharedInstance {
    pub(super) const fn new(
        instance: MySqlSharedInstancePlan,
        projects: Vec<MySqlProjectResources>,
    ) -> Self {
        Self { instance, projects }
    }

    pub(crate) const fn instance(&self) -> &MySqlSharedInstancePlan {
        &self.instance
    }

    pub(crate) fn projects(&self) -> &[MySqlProjectResources] {
        &self.projects
    }

    pub(crate) fn logical_record(
        &self,
        project: &MySqlProjectResources,
        shared: &SharedServiceReconcileResult,
    ) -> LogicalResourceRecord {
        let shared_resource_id = shared
            .volume()
            .map(|volume| volume.volume().name())
            .unwrap_or_else(|| shared.container().id().as_str());
        LogicalResourceRecord::new(LogicalResourceRecordOptions {
            logical_resource_id: project.logical().schema_name().to_owned(),
            shared_resource_id: shared_resource_id.to_owned(),
            project_id: project.environment().project_id().to_owned(),
            service_id: project.credential().service_id().to_owned(),
            kind: format!("{}_database", self.instance.flavor().implementation()),
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
