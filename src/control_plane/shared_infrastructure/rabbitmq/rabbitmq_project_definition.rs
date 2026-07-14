use super::{RabbitMqPasswordHash, RabbitMqPlanError};
use crate::control_plane::DnsLabel;
use crate::control_plane::shared_infrastructure::CredentialSecret;
use crate::control_plane::state::CredentialRecord;
use std::fmt::{Debug, Formatter};

const RABBITMQ_NAME_BYTES: usize = 128;

/// One project vhost, restricted application user, and permission binding.
#[derive(Clone)]
pub(crate) struct RabbitMqProjectDefinition {
    username: String,
    vhost: String,
    password_hash: RabbitMqPasswordHash,
}

impl RabbitMqProjectDefinition {
    pub(crate) fn new(
        project_id: &str,
        service_id: &str,
        secret: CredentialSecret,
    ) -> Result<Self, RabbitMqPlanError> {
        let project_id = DnsLabel::new("project", project_id)
            .map_err(|error| RabbitMqPlanError::new(error.to_string()))?;
        let service_id = DnsLabel::new("service", service_id)
            .map_err(|error| RabbitMqPlanError::new(error.to_string()))?;
        if secret.expose().is_empty() || secret.expose().contains('\0') {
            return Err(RabbitMqPlanError::new(
                "RabbitMQ credentials must be non-empty and contain no NUL bytes",
            ));
        }
        let identity = format!(
            "{}_{}",
            project_id.as_str().replace('-', "_"),
            service_id.as_str().replace('-', "_")
        );
        let username = format!("st_{identity}");
        let vhost = format!("stackctl_{identity}");
        for (kind, value) in [("user", &username), ("vhost", &vhost)] {
            if value.len() > RABBITMQ_NAME_BYTES {
                return Err(RabbitMqPlanError::new(format!(
                    "RabbitMQ {kind} name '{value}' exceeds {RABBITMQ_NAME_BYTES} bytes"
                )));
            }
        }
        let password_hash = RabbitMqPasswordHash::for_credential(&username, secret);

        Ok(Self {
            username,
            vhost,
            password_hash,
        })
    }

    pub(crate) fn username(&self) -> &str {
        &self.username
    }

    pub(crate) fn vhost(&self) -> &str {
        &self.vhost
    }

    pub(crate) fn matches_credential(&self, credential: &CredentialRecord) -> bool {
        credential.username() == self.username
            && RabbitMqPasswordHash::for_credential(
                &self.username,
                CredentialSecret::new(credential.secret().to_owned()),
            ) == self.password_hash
    }

    pub(crate) const fn password_hash(&self) -> &RabbitMqPasswordHash {
        &self.password_hash
    }
}

impl Debug for RabbitMqProjectDefinition {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RabbitMqProjectDefinition")
            .field("username", &self.username)
            .field("vhost", &self.vhost)
            .field("password_hash", &"[REDACTED]")
            .finish()
    }
}
