use super::{MongoDbProjectResources, MongoDbSharedInstancePlan};
use crate::control_plane::shared_infrastructure::SharedServiceReconcileResult;
use crate::control_plane::state::{
    LogicalResourceRecord, LogicalResourceRecordOptions, ResourceLifecycle,
};
use std::path::{Path, PathBuf};

/// One MongoDB process, its bootstrap secret path, and every isolated tenant.
pub(crate) struct PreparedMongoDbSharedInstance {
    instance: MongoDbSharedInstancePlan,
    projects: Vec<MongoDbProjectResources>,
    bootstrap_secret_file: PathBuf,
}

impl PreparedMongoDbSharedInstance {
    pub(super) const fn new(
        instance: MongoDbSharedInstancePlan,
        projects: Vec<MongoDbProjectResources>,
        bootstrap_secret_file: PathBuf,
    ) -> Self {
        Self {
            instance,
            projects,
            bootstrap_secret_file,
        }
    }

    pub(crate) const fn instance(&self) -> &MongoDbSharedInstancePlan {
        &self.instance
    }

    pub(crate) fn projects(&self) -> &[MongoDbProjectResources] {
        &self.projects
    }

    pub(crate) fn bootstrap_secret_file(&self) -> &Path {
        &self.bootstrap_secret_file
    }

    pub(crate) fn logical_record(
        &self,
        project: &MongoDbProjectResources,
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
                .expect("project MongoDB credential owner")
                .to_owned(),
            service_id: project.credential().service_id().to_owned(),
            kind: "mongodb_database".to_owned(),
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
