use std::fmt::{Debug, Formatter};

/// Secret-bearing credential used only to read an accepted v7 MongoDB source.
pub(crate) struct V7MongoDbCredential {
    username: String,
    password: String,
    authentication_database: String,
}

impl V7MongoDbCredential {
    pub(crate) fn new(
        username: impl Into<String>,
        password: impl Into<String>,
        authentication_database: impl Into<String>,
    ) -> Result<Self, String> {
        let username = username.into();
        let password = password.into();
        let authentication_database = authentication_database.into();
        let invalid = username.is_empty()
            || username.contains('\0')
            || password.contains('\0')
            || authentication_database.is_empty()
            || authentication_database.contains('\0');
        if invalid {
            return Err(
                "v7 MongoDB credential requires a non-empty username and authentication database with NUL-safe values"
                    .to_owned(),
            );
        }

        Ok(Self {
            username,
            password,
            authentication_database,
        })
    }

    pub(crate) fn username(&self) -> &str {
        &self.username
    }

    pub(crate) fn password(&self) -> &str {
        &self.password
    }

    pub(crate) fn authentication_database(&self) -> &str {
        &self.authentication_database
    }
}

impl Debug for V7MongoDbCredential {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("V7MongoDbCredential")
            .field("username", &self.username)
            .field("password", &"[REDACTED]")
            .field("authentication_database", &self.authentication_database)
            .finish()
    }
}
