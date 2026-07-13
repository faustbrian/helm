use super::MailpitPlanError;
use crate::control_plane::DnsLabel;
use crate::control_plane::shared_infrastructure::CredentialSecret;
use bcrypt::{Version, hash_with_salt};
use sha2::{Digest, Sha256};
use std::fmt::{Debug, Formatter};

const BCRYPT_COST: u32 = 10;
const BCRYPT_MAXIMUM_PASSWORD_BYTES: usize = 72;

/// One attributed Mailpit SMTP identity with a deterministic bcrypt hash.
#[derive(Clone)]
pub(crate) struct MailpitProjectDefinition {
    username: String,
    password_hash: String,
}

impl MailpitProjectDefinition {
    pub(crate) fn new(
        project_id: &str,
        service_id: &str,
        secret: CredentialSecret,
    ) -> Result<Self, MailpitPlanError> {
        let project_id = DnsLabel::new("project", project_id)
            .map_err(|error| MailpitPlanError::new(error.to_string()))?;
        let service_id = DnsLabel::new("service", service_id)
            .map_err(|error| MailpitPlanError::new(error.to_string()))?;
        if secret.expose().is_empty()
            || secret.expose().contains('\0')
            || secret.expose().len() > BCRYPT_MAXIMUM_PASSWORD_BYTES
        {
            return Err(MailpitPlanError::new(format!(
                "Mailpit credentials must contain 1 to {BCRYPT_MAXIMUM_PASSWORD_BYTES} bytes and no NUL bytes"
            )));
        }
        let identity = format!(
            "{}_{}",
            project_id.as_str().replace('-', "_"),
            service_id.as_str().replace('-', "_")
        );
        let username = format!("st_{identity}");
        let salt = deterministic_salt(&username, secret.expose());
        let password_hash = hash_with_salt(secret.expose(), BCRYPT_COST, salt)
            .map_err(|error| {
                MailpitPlanError::new(format!("failed to hash Mailpit credential: {error}"))
            })?
            .format_for_version(Version::TwoB);

        Ok(Self {
            username,
            password_hash,
        })
    }

    pub(crate) fn username(&self) -> &str {
        &self.username
    }

    pub(super) fn password_hash(&self) -> &str {
        &self.password_hash
    }
}

impl Debug for MailpitProjectDefinition {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MailpitProjectDefinition")
            .field("username", &self.username)
            .field("password_hash", &"[REDACTED]")
            .finish()
    }
}

fn deterministic_salt(username: &str, secret: &str) -> [u8; 16] {
    let mut hasher = Sha256::new();
    hasher.update(b"stackctl-mailpit-bcrypt-salt\0");
    hasher.update(username.as_bytes());
    hasher.update(b"\0");
    hasher.update(secret.as_bytes());
    let digest = hasher.finalize();
    let mut salt = [0_u8; 16];
    for (target, source) in salt.iter_mut().zip(digest.iter()) {
        *target = *source;
    }
    salt
}
