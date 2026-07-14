use std::fmt::{Debug, Formatter};

/// Secret-bearing credential used only to read an accepted v7 PostgreSQL source.
pub(crate) struct V7PostgresCredential {
    username: String,
    password: String,
}

impl V7PostgresCredential {
    pub(crate) fn new(
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Result<Self, String> {
        let username = username.into();
        let password = password.into();
        if username.is_empty() || username.contains('\0') || password.contains('\0') {
            return Err(
                "v7 PostgreSQL credential requires a non-empty username and NUL-safe password"
                    .to_owned(),
            );
        }

        Ok(Self { username, password })
    }

    pub(crate) fn username(&self) -> &str {
        &self.username
    }

    pub(crate) fn password(&self) -> &str {
        &self.password
    }
}

impl Debug for V7PostgresCredential {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("V7PostgresCredential")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}
