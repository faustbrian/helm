use super::RedisPlanError;
use crate::control_plane::DnsLabel;
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::state::CredentialRecord;
use sha2::{Digest, Sha256};
use std::fmt::{Debug, Formatter};

/// One project-scoped user in a complete Redis-compatible ACL snapshot.
#[derive(Clone)]
pub(crate) struct RedisAclProject {
    username: String,
    prefix: String,
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
        let prefix = format!("stackctl:{}:{}:", project_id.as_str(), service_id.as_str());
        let pattern = format!("{prefix}*");

        Ok(Self {
            username,
            prefix,
            key_pattern: pattern.clone(),
            channel_pattern: pattern,
            secret,
        })
    }

    pub(crate) fn username(&self) -> &str {
        &self.username
    }

    pub(crate) fn prefix(&self) -> &str {
        &self.prefix
    }

    pub(crate) fn matches_credential(&self, credential: &CredentialRecord) -> bool {
        credential.username() == self.username()
            && password_hash(credential.secret()) == password_hash(self.secret.expose())
    }

    pub(super) fn acl_line(&self) -> String {
        format!(
            "user {} on resetpass #{} resetkeys ~{} resetchannels &{} -@all \
             +@read +@write +@connection +@transaction +@pubsub +@scripting \
             -@admin -@dangerous -scan -keys -randomkey",
            self.username,
            password_hash(self.secret.expose()),
            self.key_pattern,
            self.channel_pattern,
        )
    }
}

pub(super) fn password_hash(secret: &str) -> String {
    hex::encode(Sha256::digest(secret.as_bytes()))
}

impl Debug for RedisAclProject {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RedisAclProject")
            .field("username", &self.username)
            .field("prefix", &self.prefix)
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
