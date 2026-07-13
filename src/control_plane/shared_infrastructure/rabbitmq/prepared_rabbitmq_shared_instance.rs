use super::{RabbitMqDefinitions, RabbitMqProjectResources, RabbitMqSharedInstancePlan};
use crate::control_plane::shared_infrastructure::SharedServiceReconcileResult;
use crate::control_plane::state::{
    LogicalResourceRecord, LogicalResourceRecordOptions, ResourceLifecycle,
};
use std::path::{Path, PathBuf};

/// One broker process, complete definitions snapshot, and isolated project vhosts.
pub(crate) struct PreparedRabbitMqSharedInstance {
    instance: RabbitMqSharedInstancePlan,
    projects: Vec<RabbitMqProjectResources>,
    definitions: RabbitMqDefinitions,
    state_directory: PathBuf,
}

impl PreparedRabbitMqSharedInstance {
    pub(super) const fn new(
        instance: RabbitMqSharedInstancePlan,
        projects: Vec<RabbitMqProjectResources>,
        definitions: RabbitMqDefinitions,
        state_directory: PathBuf,
    ) -> Self {
        Self {
            instance,
            projects,
            definitions,
            state_directory,
        }
    }

    pub(crate) const fn instance(&self) -> &RabbitMqSharedInstancePlan {
        &self.instance
    }

    pub(crate) fn projects(&self) -> &[RabbitMqProjectResources] {
        &self.projects
    }

    pub(crate) const fn definitions(&self) -> &RabbitMqDefinitions {
        &self.definitions
    }

    pub(crate) fn state_directory(&self) -> &Path {
        &self.state_directory
    }

    pub(crate) fn logical_record(
        &self,
        project: &RabbitMqProjectResources,
        shared: &SharedServiceReconcileResult,
    ) -> LogicalResourceRecord {
        let shared_resource_id = shared
            .volume()
            .map(|volume| volume.volume().name())
            .unwrap_or_else(|| shared.container().id().as_str());
        LogicalResourceRecord::new(LogicalResourceRecordOptions {
            logical_resource_id: project.credential().credential_id().to_owned(),
            shared_resource_id: shared_resource_id.to_owned(),
            project_id: project
                .credential()
                .project_id()
                .expect("project RabbitMQ credential owner")
                .to_owned(),
            service_id: project.credential().service_id().to_owned(),
            kind: "rabbitmq_vhost_user".to_owned(),
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
