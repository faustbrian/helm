use super::redis_acl_project::{password_hash, validate_secret};
use super::{RedisAclProject, RedisPlanError};
use crate::control_plane::shared_infrastructure::CredentialSecret;
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};

/// Complete Redis-compatible ACL state, rendered in deterministic user order.
pub(crate) struct RedisAclSnapshot {
    contents: String,
    user_count: usize,
}

impl RedisAclSnapshot {
    pub(crate) fn new(
        admin_secret: CredentialSecret,
        projects: Vec<RedisAclProject>,
    ) -> Result<Self, RedisPlanError> {
        validate_secret(admin_secret.expose())?;
        let mut users = BTreeMap::new();
        for project in projects {
            let username = project.username().to_owned();
            if users.insert(username.clone(), project).is_some() {
                return Err(RedisPlanError::new(format!(
                    "Redis ACL user '{username}' is defined more than once"
                )));
            }
        }

        let mut contents = format!(
            "user default off resetpass resetkeys resetchannels -@all\n\
             user stackctl_admin on resetpass #{} resetkeys ~* resetchannels &* +@all\n",
            password_hash(admin_secret.expose())
        );
        for project in users.values() {
            contents.push_str(&project.acl_line());
            contents.push('\n');
        }

        Ok(Self {
            contents,
            user_count: users.len() + 1,
        })
    }

    pub(crate) fn contents(&self) -> &str {
        &self.contents
    }
}

impl Debug for RedisAclSnapshot {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RedisAclSnapshot")
            .field("user_count", &self.user_count)
            .finish()
    }
}
