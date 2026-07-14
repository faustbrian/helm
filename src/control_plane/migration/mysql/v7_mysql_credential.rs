use std::fmt::{Debug, Formatter};

/// Secret-bearing credential used only to read an accepted v7 MySQL source.
pub(crate) struct V7MySqlCredential {
    username: String,
    password: String,
}

impl V7MySqlCredential {
    pub(crate) fn new(
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Result<Self, String> {
        let username = username.into();
        let password = password.into();
        if username.is_empty() || username.contains('\0') || password.contains('\0') {
            return Err(
                "v7 MySQL-family credential requires a non-empty username and NUL-safe password"
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

impl Debug for V7MySqlCredential {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("V7MySqlCredential")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}
