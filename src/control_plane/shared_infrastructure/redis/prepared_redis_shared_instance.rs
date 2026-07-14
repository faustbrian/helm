use super::{RedisAclSnapshot, RedisProjectResources, RedisSharedInstancePlan};
use crate::control_plane::shared_infrastructure::SharedServiceReconcileResult;
use crate::control_plane::state::{
    LogicalResourceRecord, LogicalResourceRecordOptions, ResourceLifecycle,
};
use std::path::{Path, PathBuf};

/// One Redis-compatible process, complete ACL snapshot, and project namespaces.
pub(crate) struct PreparedRedisSharedInstance {
    instance: RedisSharedInstancePlan,
    projects: Vec<RedisProjectResources>,
    snapshot: RedisAclSnapshot,
    state_directory: PathBuf,
}

impl PreparedRedisSharedInstance {
    pub(super) const fn new(
        instance: RedisSharedInstancePlan,
        projects: Vec<RedisProjectResources>,
        snapshot: RedisAclSnapshot,
        state_directory: PathBuf,
    ) -> Self {
        Self {
            instance,
            projects,
            snapshot,
            state_directory,
        }
    }

    pub(crate) const fn instance(&self) -> &RedisSharedInstancePlan {
        &self.instance
    }

    pub(crate) fn projects(&self) -> &[RedisProjectResources] {
        &self.projects
    }

    pub(crate) const fn snapshot(&self) -> &RedisAclSnapshot {
        &self.snapshot
    }

    pub(crate) fn state_directory(&self) -> &Path {
        &self.state_directory
    }

    pub(crate) fn logical_record(
        &self,
        project: &RedisProjectResources,
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
            kind: format!("{}_acl_prefix", self.instance.flavor().implementation()),
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
