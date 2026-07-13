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
    /// Installation identity or Engine selection differs from durable state.
    InstallationAlreadyInitialized { existing_installation_id: String },
    /// A credential identity is already owned by a different project service.
    CredentialOwnershipConflict { credential_id: String },
    /// A resource identity cannot silently change immutable ownership metadata.
    ResourceOwnershipConflict { resource_id: String },
    /// A logical tenant identity cannot silently change shared ownership metadata.
    LogicalResourceOwnershipConflict { logical_resource_id: String },
    /// Generic reconciliation cannot reactivate retained project data.
    ResourceAdoptionRequired { resource_id: String },
    /// Disabled project state cannot be reactivated by ordinary reconciliation.
    ProjectAdoptionRequired { project_id: String },
    /// A project adoption plan is structurally unsafe.
    InvalidProjectAdoption { detail: String },
    /// The requested adoption target is not the registered project path.
    ProjectAdoptionTargetMissing { project_id: String, path: PathBuf },
    /// Durable project state does not exactly match the adoption plan.
    ProjectAdoptionStateMismatch { project_id: String, detail: String },
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
            Self::InstallationAlreadyInitialized {
                existing_installation_id,
            } => write!(
                formatter,
                "Stackctl installation is already initialized as '{existing_installation_id}'; explicit migration is required"
            ),
            Self::CredentialOwnershipConflict { credential_id } => write!(
                formatter,
                "credential '{credential_id}' is already owned by a different project service"
            ),
            Self::ResourceOwnershipConflict { resource_id } => write!(
                formatter,
                "resource '{resource_id}' has immutable ownership metadata that differs from durable state; explicit adoption or migration is required"
            ),
            Self::LogicalResourceOwnershipConflict {
                logical_resource_id,
            } => write!(
                formatter,
                "logical resource '{logical_resource_id}' has immutable ownership metadata that differs from durable state; explicit adoption or migration is required"
            ),
            Self::ResourceAdoptionRequired { resource_id } => write!(
                formatter,
                "resource '{resource_id}' is orphaned or retained; explicit adoption is required before reactivation"
            ),
            Self::ProjectAdoptionRequired { project_id } => write!(
                formatter,
                "project '{project_id}' has disabled managed state; explicit adoption is required before reactivation"
            ),
            Self::InvalidProjectAdoption { detail } => {
                write!(formatter, "invalid project adoption plan: {detail}")
            }
            Self::ProjectAdoptionTargetMissing { project_id, path } => write!(
                formatter,
                "project '{project_id}' cannot be adopted at '{}' because that exact target is not registered",
                path.display()
            ),
            Self::ProjectAdoptionStateMismatch { project_id, detail } => write!(
                formatter,
                "project '{project_id}' adoption does not match retained state: {detail}"
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
            | Self::InstallationAlreadyInitialized { .. }
            | Self::CredentialOwnershipConflict { .. }
            | Self::ResourceOwnershipConflict { .. }
            | Self::LogicalResourceOwnershipConflict { .. }
            | Self::ResourceAdoptionRequired { .. }
            | Self::ProjectAdoptionRequired { .. }
            | Self::InvalidProjectAdoption { .. }
            | Self::ProjectAdoptionTargetMissing { .. }
            | Self::ProjectAdoptionStateMismatch { .. }
            | Self::CorruptState { .. } => None,
        }
    }
}

impl From<rusqlite::Error> for StateStoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}
