use super::RedisAclProject;
use crate::control_plane::state::{CredentialRecord, ManagedEnvironmentRecord};

/// Complete durable cache resources for one project.
#[derive(Debug)]
pub(crate) struct RedisProjectResources {
    acl: RedisAclProject,
    credential: CredentialRecord,
    environment: ManagedEnvironmentRecord,
}

impl RedisProjectResources {
    pub(super) const fn new(
        acl: RedisAclProject,
        credential: CredentialRecord,
        environment: ManagedEnvironmentRecord,
    ) -> Self {
        Self {
            acl,
            credential,
            environment,
        }
    }

    pub(crate) const fn acl(&self) -> &RedisAclProject {
        &self.acl
    }

    pub(crate) const fn credential(&self) -> &CredentialRecord {
        &self.credential
    }

    pub(crate) const fn environment(&self) -> &ManagedEnvironmentRecord {
        &self.environment
    }
}
