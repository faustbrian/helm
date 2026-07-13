use super::{PostgresProjectResources, PostgresSharedInstancePlan};
use crate::control_plane::shared_infrastructure::SharedServiceReconcileResult;
use crate::control_plane::state::{
    LogicalResourceRecord, LogicalResourceRecordOptions, ResourceLifecycle,
};

/// One physical PostgreSQL plan and every isolated project tenant it owns.
pub(crate) struct PreparedPostgresSharedInstance {
    instance: PostgresSharedInstancePlan,
    projects: Vec<PostgresProjectResources>,
}

impl PreparedPostgresSharedInstance {
    pub(crate) const fn new(
        instance: PostgresSharedInstancePlan,
        projects: Vec<PostgresProjectResources>,
    ) -> Self {
        Self { instance, projects }
    }

    pub(crate) const fn instance(&self) -> &PostgresSharedInstancePlan {
        &self.instance
    }

    pub(crate) fn projects(&self) -> &[PostgresProjectResources] {
        &self.projects
    }

    pub(crate) fn logical_record(
        &self,
        project: &PostgresProjectResources,
        shared: &SharedServiceReconcileResult,
    ) -> LogicalResourceRecord {
        let shared_resource_id = shared
            .volume()
            .map(|volume| volume.volume().name())
            .unwrap_or_else(|| shared.container().id().as_str());
        LogicalResourceRecord::new(LogicalResourceRecordOptions {
            logical_resource_id: project.logical().database_name().to_owned(),
            shared_resource_id: shared_resource_id.to_owned(),
            project_id: project.logical().project_id().to_owned(),
            service_id: project.logical().service_id().to_owned(),
            kind: "postgres_database_and_role".to_owned(),
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
