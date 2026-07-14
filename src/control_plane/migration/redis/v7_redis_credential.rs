use std::fmt::{Debug, Formatter};

/// Credential and logical database used only for an accepted v7 keyspace.
pub(crate) struct V7RedisCredential {
    username: String,
    password: String,
    database: u32,
}

impl V7RedisCredential {
    pub(crate) fn new(
        username: impl Into<String>,
        password: impl Into<String>,
        database: u32,
    ) -> Result<Self, String> {
        let username = username.into();
        let password = password.into();
        if username.is_empty()
            || username.contains('\0')
            || password.contains('\0')
            || username.chars().any(char::is_whitespace)
        {
            return Err(
                "v7 Redis-compatible credential requires a non-empty token username and NUL-safe password"
                    .to_owned(),
            );
        }
        Ok(Self {
            username,
            password,
            database,
        })
    }

    pub(crate) fn username(&self) -> &str {
        &self.username
    }

    pub(crate) fn password(&self) -> &str {
        &self.password
    }

    pub(crate) const fn database(&self) -> u32 {
        self.database
    }
}

impl Debug for V7RedisCredential {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("V7RedisCredential")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .field("database", &self.database)
            .finish()
    }
}
