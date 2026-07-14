use std::fmt::{Debug, Formatter};

/// Exact access identity used only for one accepted v7 MinIO bucket.
pub(crate) struct V7MinioCredential {
    access_key: String,
    secret_key: String,
}

impl V7MinioCredential {
    pub(crate) fn new(
        access_key: impl Into<String>,
        secret_key: impl Into<String>,
    ) -> Result<Self, String> {
        let access_key = access_key.into();
        let secret_key = secret_key.into();
        if access_key.is_empty()
            || secret_key.is_empty()
            || access_key.contains('\0')
            || secret_key.contains('\0')
            || access_key.chars().any(char::is_whitespace)
        {
            return Err(
                "v7 MinIO credential requires a non-empty token access key and NUL-safe secret"
                    .to_owned(),
            );
        }
        Ok(Self {
            access_key,
            secret_key,
        })
    }

    pub(crate) fn access_key(&self) -> &str {
        &self.access_key
    }

    pub(crate) fn secret_key(&self) -> &str {
        &self.secret_key
    }
}

impl Debug for V7MinioCredential {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("V7MinioCredential")
            .field("access_key", &"[REDACTED]")
            .field("secret_key", &"[REDACTED]")
            .finish()
    }
}
