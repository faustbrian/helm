use crate::control_plane::shared_infrastructure::CredentialSecret;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use sha2::{Digest, Sha256};
use std::fmt::{Debug, Formatter};

/// RabbitMQ's base64-encoded four-byte-salt plus SHA-256 credential hash.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct RabbitMqPasswordHash {
    encoded: String,
}

impl RabbitMqPasswordHash {
    pub(crate) fn for_credential(username: &str, secret: CredentialSecret) -> Self {
        let mut salt_seed = Sha256::new();
        salt_seed.update(b"stackctl-rabbitmq-salt\0");
        salt_seed.update(username.as_bytes());
        salt_seed.update(b"\0");
        salt_seed.update(secret.expose().as_bytes());
        let digest = salt_seed.finalize();
        let mut salt = [0_u8; 4];
        for (target, source) in salt.iter_mut().zip(digest.iter()) {
            *target = *source;
        }

        Self::from_salt(secret, salt)
    }

    pub(crate) fn from_salt(secret: CredentialSecret, salt: [u8; 4]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(salt);
        hasher.update(secret.expose().as_bytes());
        let digest = hasher.finalize();
        let mut salted_hash = Vec::with_capacity(salt.len() + digest.len());
        salted_hash.extend_from_slice(&salt);
        salted_hash.extend_from_slice(&digest);

        Self {
            encoded: STANDARD.encode(salted_hash),
        }
    }

    pub(crate) fn encoded(&self) -> &str {
        &self.encoded
    }
}

impl Debug for RabbitMqPasswordHash {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RabbitMqPasswordHash([REDACTED])")
    }
}
