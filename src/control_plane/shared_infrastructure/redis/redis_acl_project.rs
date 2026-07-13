use super::RedisPlanError;
use crate::control_plane::DnsLabel;
use crate::control_plane::shared_infrastructure::CredentialSecret;
use std::fmt::{Debug, Formatter};

/// One project-scoped user in a complete Redis-compatible ACL snapshot.
pub(crate) struct RedisAclProject {
    username: String,
    key_pattern: String,
    channel_pattern: String,
    secret: CredentialSecret,
}

impl RedisAclProject {
    pub(crate) fn new(
        project_id: &str,
        service_id: &str,
        secret: CredentialSecret,
    ) -> Result<Self, RedisPlanError> {
        let project_id = DnsLabel::new("project", project_id)
            .map_err(|error| RedisPlanError::new(error.to_string()))?;
        let service_id = DnsLabel::new("service", service_id)
            .map_err(|error| RedisPlanError::new(error.to_string()))?;
        validate_secret(secret.expose())?;
        let username = format!(
            "st_{}_{}",
            project_id.as_str().replace('-', "_"),
            service_id.as_str().replace('-', "_")
        );
        let prefix = format!("stackctl:{}:{}:*", project_id.as_str(), service_id.as_str());

        Ok(Self {
            username,
            key_pattern: prefix.clone(),
            channel_pattern: prefix,
            secret,
        })
    }

    pub(crate) fn username(&self) -> &str {
        &self.username
    }

    pub(super) fn acl_line(&self) -> String {
        format!(
            "user {} on resetpass >{} resetkeys ~{} resetchannels &{} -@all \
             +@read +@write +@connection +@transaction +@pubsub +@scripting",
            self.username,
            self.secret.expose(),
            self.key_pattern,
            self.channel_pattern,
        )
    }
}

impl Debug for RedisAclProject {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RedisAclProject")
            .field("username", &self.username)
            .field("key_pattern", &self.key_pattern)
            .field("channel_pattern", &self.channel_pattern)
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

pub(super) fn validate_secret(secret: &str) -> Result<(), RedisPlanError> {
    if secret.is_empty() || secret.chars().any(char::is_whitespace) || secret.contains('\0') {
        return Err(RedisPlanError::new(
            "Redis ACL secrets must be non-empty tokens without whitespace or NUL bytes",
        ));
    }

    Ok(())
}
