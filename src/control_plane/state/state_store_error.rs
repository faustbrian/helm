use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;

/// A durable state migration, validation, or transaction failure.
#[derive(Debug)]
#[non_exhaustive]
pub(crate) enum StateStoreError {
    /// SQLite rejected an operation.
    Database(rusqlite::Error),
    /// A persisted database uses a schema newer than this build.
    UnsupportedSchema { found: u32, supported: u32 },
    /// A canonical path cannot be represented exactly in SQLite text.
    NonUtf8Path { path: PathBuf },
    /// A route is already durably owned by a different canonical path.
    RouteOwnershipConflict {
        domain: String,
        existing_path: PathBuf,
        requested_path: PathBuf,
    },
    /// Persisted state contains a value outside the supported typed model.
    CorruptState { detail: String },
}

impl Display for StateStoreError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "state database error: {error}"),
            Self::UnsupportedSchema { found, supported } => write!(
                formatter,
                "state schema version {found} is newer than supported version {supported}"
            ),
            Self::NonUtf8Path { path } => write!(
                formatter,
                "canonical project path '{}' is not valid UTF-8",
                path.display()
            ),
            Self::RouteOwnershipConflict {
                domain,
                existing_path,
                requested_path,
            } => write!(
                formatter,
                "route '{domain}' is owned by '{}', not '{}'",
                existing_path.display(),
                requested_path.display()
            ),
            Self::CorruptState { detail } => {
                write!(formatter, "state database contains invalid data: {detail}")
            }
        }
    }
}

impl Error for StateStoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::UnsupportedSchema { .. }
            | Self::NonUtf8Path { .. }
            | Self::RouteOwnershipConflict { .. }
            | Self::CorruptState { .. } => None,
        }
    }
}

impl From<rusqlite::Error> for StateStoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}
