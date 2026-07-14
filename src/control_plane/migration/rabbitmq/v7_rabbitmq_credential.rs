use crate::control_plane::shared_infrastructure::{CredentialSecret, RabbitMqPasswordHash};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use std::fmt::{Debug, Formatter};

/// Exact application identity used only for one accepted v7 RabbitMQ vhost.
pub(crate) struct V7RabbitMqCredential {
    username: String,
    password: String,
}

impl V7RabbitMqCredential {
    pub(crate) fn new(
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Result<Self, String> {
        let username = username.into();
        let password = password.into();
        if username.is_empty()
            || password.is_empty()
            || username.contains('\0')
            || password.contains('\0')
        {
            return Err(
                "v7 RabbitMQ credential requires non-empty NUL-safe username and password"
                    .to_owned(),
            );
        }
        Ok(Self { username, password })
    }

    pub(crate) fn username(&self) -> &str {
        &self.username
    }

    pub(crate) fn matches_password_hash(&self, encoded: &str) -> bool {
        let Ok(decoded) = STANDARD.decode(encoded) else {
            return false;
        };
        let Ok(salt) = <[u8; 4]>::try_from(decoded.get(..4).unwrap_or_default()) else {
            return false;
        };
        decoded.len() == 36
            && RabbitMqPasswordHash::from_salt(CredentialSecret::new(self.password.clone()), salt)
                .encoded()
                == encoded
    }
}

impl Debug for V7RabbitMqCredential {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("V7RabbitMqCredential")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}
