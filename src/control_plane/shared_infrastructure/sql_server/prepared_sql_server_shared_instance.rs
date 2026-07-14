use super::{SqlServerProjectResources, SqlServerSharedInstancePlan};
use crate::control_plane::shared_infrastructure::SharedServiceReconcileResult;
use crate::control_plane::state::{
    LogicalResourceRecord, LogicalResourceRecordOptions, ResourceLifecycle,
};

/// One SQL Server process and every isolated project database and login.
pub(crate) struct PreparedSqlServerSharedInstance {
    instance: SqlServerSharedInstancePlan,
    projects: Vec<SqlServerProjectResources>,
}

impl PreparedSqlServerSharedInstance {
    pub(super) const fn new(
        instance: SqlServerSharedInstancePlan,
        projects: Vec<SqlServerProjectResources>,
    ) -> Self {
        Self { instance, projects }
    }

    pub(crate) const fn instance(&self) -> &SqlServerSharedInstancePlan {
        &self.instance
    }

    pub(crate) fn projects(&self) -> &[SqlServerProjectResources] {
        &self.projects
    }

    pub(crate) fn logical_record(
        &self,
        project: &SqlServerProjectResources,
        shared: &SharedServiceReconcileResult,
    ) -> LogicalResourceRecord {
        let shared_resource_id = shared
            .volume()
            .map(|volume| volume.volume().name())
            .unwrap_or_else(|| shared.container().id().as_str());
        LogicalResourceRecord::new(LogicalResourceRecordOptions {
            logical_resource_id: project.credential().credential_id().to_owned(),
            shared_resource_id: shared_resource_id.to_owned(),
            project_id: project.environment().project_id().to_owned(),
            service_id: project.credential().service_id().to_owned(),
            kind: "sqlserver_database".to_owned(),
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
