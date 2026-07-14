use crate::control_plane::state::{LogicalResourceRecord, ResourceRecord};

/// Exact durable resource identity protected by one backup artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BackupResourceIdentity {
    resource_id: String,
    installation_id: String,
    resource_kind: String,
    compatibility_fingerprint: String,
}

impl BackupResourceIdentity {
    pub(crate) fn for_v7_generated_environment(project_id: &str, evidence_revision: &str) -> Self {
        Self {
            resource_id: project_id.to_owned(),
            installation_id: "v7-migration".to_owned(),
            resource_kind: "generated_environment".to_owned(),
            compatibility_fingerprint: evidence_revision.to_owned(),
        }
    }

    pub(crate) fn for_v7_gateway_snapshot(project_id: &str, evidence_revision: &str) -> Self {
        Self {
            resource_id: project_id.to_owned(),
            installation_id: "v7-migration".to_owned(),
            resource_kind: "gateway_snapshot".to_owned(),
            compatibility_fingerprint: evidence_revision.to_owned(),
        }
    }

    pub(crate) fn from_resource(resource: &ResourceRecord) -> Self {
        Self {
            resource_id: resource.resource_id().to_owned(),
            installation_id: resource.installation_id().to_owned(),
            resource_kind: resource.kind().to_owned(),
            compatibility_fingerprint: resource.compatibility_fingerprint().to_owned(),
        }
    }

    pub(crate) fn from_logical(resource: &LogicalResourceRecord, installation_id: &str) -> Self {
        Self {
            resource_id: resource.logical_resource_id().to_owned(),
            installation_id: installation_id.to_owned(),
            resource_kind: resource.kind().to_owned(),
            compatibility_fingerprint: resource.compatibility_fingerprint().to_owned(),
        }
    }

    pub(super) fn resource_id(&self) -> &str {
        &self.resource_id
    }

    pub(super) fn installation_id(&self) -> &str {
        &self.installation_id
    }

    pub(super) fn resource_kind(&self) -> &str {
        &self.resource_kind
    }

    pub(super) fn compatibility_fingerprint(&self) -> &str {
        &self.compatibility_fingerprint
    }
}
