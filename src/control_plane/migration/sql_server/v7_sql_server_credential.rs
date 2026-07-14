use std::fmt::{Debug, Formatter};

/// Secret-bearing administrator used only for an accepted v7 SQL Server.
pub(crate) struct V7SqlServerCredential {
    username: String,
    password: String,
}

impl V7SqlServerCredential {
    pub(crate) fn new(
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Result<Self, String> {
        let username = username.into();
        let password = password.into();
        if username.is_empty()
            || username.contains('\0')
            || password.is_empty()
            || password.contains('\0')
        {
            return Err("v7 SQL Server credential requires non-empty NUL-safe values".to_owned());
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

impl Debug for V7SqlServerCredential {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("V7SqlServerCredential")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .finish()
    }
}
