use super::{ObjectStoreProjectResources, ObjectStoreSharedInstancePlan};
use crate::control_plane::shared_infrastructure::SharedServiceReconcileResult;
use crate::control_plane::state::{
    LogicalResourceRecord, LogicalResourceRecordOptions, ResourceLifecycle,
};
use std::path::{Path, PathBuf};

/// One object-store process and every isolated project bucket and identity.
pub(crate) struct PreparedObjectStoreSharedInstance {
    instance: ObjectStoreSharedInstancePlan,
    projects: Vec<ObjectStoreProjectResources>,
    policy_directory: PathBuf,
}

impl PreparedObjectStoreSharedInstance {
    pub(super) const fn new(
        instance: ObjectStoreSharedInstancePlan,
        projects: Vec<ObjectStoreProjectResources>,
        policy_directory: PathBuf,
    ) -> Self {
        Self {
            instance,
            projects,
            policy_directory,
        }
    }

    pub(crate) const fn instance(&self) -> &ObjectStoreSharedInstancePlan {
        &self.instance
    }

    pub(crate) fn projects(&self) -> &[ObjectStoreProjectResources] {
        &self.projects
    }

    pub(crate) fn policy_directory(&self) -> &Path {
        &self.policy_directory
    }

    pub(crate) fn logical_record(
        &self,
        project: &ObjectStoreProjectResources,
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
                .expect("project object-store credential owner")
                .to_owned(),
            service_id: project.credential().service_id().to_owned(),
            kind: format!("{}_bucket_policy", self.instance.flavor().implementation()),
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
