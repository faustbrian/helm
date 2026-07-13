use super::{MailpitAuthenticationSnapshot, MailpitProjectResources, MailpitSharedInstancePlan};
use crate::control_plane::gateway::GatewayRoute;
use crate::control_plane::shared_infrastructure::SharedServiceReconcileResult;
use crate::control_plane::state::{
    LogicalResourceRecord, LogicalResourceRecordOptions, ResourceLifecycle,
};
use std::path::{Path, PathBuf};

/// One Mailpit process, complete SMTP authentication, and attributed projects.
pub(crate) struct PreparedMailpitSharedInstance {
    instance: MailpitSharedInstancePlan,
    projects: Vec<MailpitProjectResources>,
    snapshot: MailpitAuthenticationSnapshot,
    state_directory: PathBuf,
}

impl PreparedMailpitSharedInstance {
    pub(super) const fn new(
        instance: MailpitSharedInstancePlan,
        projects: Vec<MailpitProjectResources>,
        snapshot: MailpitAuthenticationSnapshot,
        state_directory: PathBuf,
    ) -> Self {
        Self {
            instance,
            projects,
            snapshot,
            state_directory,
        }
    }

    pub(crate) const fn instance(&self) -> &MailpitSharedInstancePlan {
        &self.instance
    }

    pub(crate) fn projects(&self) -> &[MailpitProjectResources] {
        &self.projects
    }

    pub(crate) const fn snapshot(&self) -> &MailpitAuthenticationSnapshot {
        &self.snapshot
    }

    pub(crate) fn state_directory(&self) -> &Path {
        &self.state_directory
    }

    pub(crate) fn routes(&self) -> Vec<GatewayRoute> {
        self.projects
            .iter()
            .map(|project| project.route().clone())
            .collect()
    }

    pub(crate) fn logical_record(
        &self,
        project: &MailpitProjectResources,
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
                .expect("project Mailpit credential owner")
                .to_owned(),
            service_id: project.credential().service_id().to_owned(),
            kind: "mailpit_smtp_identity".to_owned(),
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
