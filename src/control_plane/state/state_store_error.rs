use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io;
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
    /// Logical tenant ownership and its generated environment disagree.
    InvalidLogicalEnvironment { detail: String },
    /// Generic reconciliation cannot reactivate retained project data.
    ResourceAdoptionRequired { resource_id: String },
    /// Backend deletion cannot retire active or changed durable ownership.
    InvalidResourceRetirement { resource_id: String, detail: String },
    /// Logical deletion cannot retire active, incomplete, or changed state.
    InvalidLogicalResourceRetirement {
        logical_resource_id: String,
        detail: String,
    },
    /// Disabled project state cannot be reactivated by ordinary reconciliation.
    ProjectAdoptionRequired { project_id: String },
    /// A project adoption plan is structurally unsafe.
    InvalidProjectAdoption { detail: String },
    /// A migration identity cannot silently change after inventory.
    MigrationIdentityConflict { migration_id: String },
    /// Explicitly accepted legacy source evidence cannot change implicitly.
    AcceptedV7InventoryConflict { path: PathBuf },
    /// Legacy project identity and canonical path cannot be repaired implicitly.
    AcceptedV7ProjectIdentityConflict {
        project_id: String,
        existing_project_id: String,
        existing_path: PathBuf,
        requested_path: PathBuf,
    },
    /// Backup, target, or rollback evidence changed after being recorded.
    MigrationEvidenceConflict { migration_id: String },
    /// Verified recovery evidence cannot change after publication.
    RecoveryPointEvidenceConflict { recovery_point_id: String },
    /// A migration checkpoint skipped, regressed, or changed a terminal phase.
    InvalidMigrationTransition {
        migration_id: String,
        from: String,
        to: String,
    },
    /// A migration checkpoint time moved backwards.
    MigrationTimeRegression { migration_id: String },
    /// A provisioned target does not match its exact durable ownership state.
    InvalidMigrationTarget { detail: String },
    /// A target credential differs from the stable value already persisted.
    MigrationTargetCredentialConflict { credential_id: String },
    /// A cutover plan does not describe one exact active project transition.
    InvalidMigrationCutover { detail: String },
    /// A rollback plan does not restore one exact project and retain its targets.
    InvalidMigrationRollback { detail: String },
    /// The requested adoption target is not the registered project path.
    ProjectAdoptionTargetMissing { project_id: String, path: PathBuf },
    /// Durable project state does not exactly match the adoption plan.
    ProjectAdoptionStateMismatch { project_id: String, detail: String },
    /// Persisted state contains a value outside the supported typed model.
    CorruptState { detail: String },
    /// A daemon event cannot be persisted without violating journal invariants.
    InvalidDaemonEvent { detail: String },
    /// A daemon operation cannot be persisted or transition safely.
    InvalidDaemonOperation { detail: String },
    /// A private state snapshot could not be created or retained safely.
    StateBackupIo {
        action: &'static str,
        path: PathBuf,
        source: io::Error,
    },
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
            Self::InvalidLogicalEnvironment { detail } => {
                write!(formatter, "invalid logical resource environment: {detail}")
            }
            Self::ResourceAdoptionRequired { resource_id } => write!(
                formatter,
                "resource '{resource_id}' is orphaned or retained; explicit adoption is required before reactivation"
            ),
            Self::InvalidResourceRetirement {
                resource_id,
                detail,
            } => write!(
                formatter,
                "resource '{resource_id}' cannot be retired: {detail}"
            ),
            Self::InvalidLogicalResourceRetirement {
                logical_resource_id,
                detail,
            } => write!(
                formatter,
                "logical resource '{logical_resource_id}' cannot be retired: {detail}"
            ),
            Self::ProjectAdoptionRequired { project_id } => write!(
                formatter,
                "project '{project_id}' has disabled managed state; explicit adoption is required before reactivation"
            ),
            Self::InvalidProjectAdoption { detail } => {
                write!(formatter, "invalid project adoption plan: {detail}")
            }
            Self::MigrationIdentityConflict { migration_id } => write!(
                formatter,
                "migration '{migration_id}' immutable identity differs from durable state"
            ),
            Self::AcceptedV7InventoryConflict { path } => write!(
                formatter,
                "accepted v7 inventory for '{}' differs from durable evidence",
                path.display()
            ),
            Self::AcceptedV7ProjectIdentityConflict {
                project_id,
                existing_project_id,
                existing_path,
                requested_path,
            } => write!(
                formatter,
                "legacy project '{project_id}' at '{}' conflicts with project '{existing_project_id}' already accepted at '{}'; rename the directory or project explicitly",
                requested_path.display(),
                existing_path.display()
            ),
            Self::MigrationEvidenceConflict { migration_id } => write!(
                formatter,
                "migration '{migration_id}' durable evidence cannot be replaced"
            ),
            Self::RecoveryPointEvidenceConflict { recovery_point_id } => write!(
                formatter,
                "recovery point '{recovery_point_id}' immutable evidence cannot be replaced"
            ),
            Self::InvalidMigrationTransition {
                migration_id,
                from,
                to,
            } => write!(
                formatter,
                "migration '{migration_id}' cannot advance from '{from}' to '{to}'"
            ),
            Self::MigrationTimeRegression { migration_id } => write!(
                formatter,
                "migration '{migration_id}' update time predates durable state"
            ),
            Self::InvalidMigrationTarget { detail } => {
                write!(formatter, "invalid migration target: {detail}")
            }
            Self::MigrationTargetCredentialConflict { credential_id } => write!(
                formatter,
                "migration target credential '{credential_id}' differs from durable state"
            ),
            Self::InvalidMigrationCutover { detail } => {
                write!(formatter, "invalid migration cutover: {detail}")
            }
            Self::InvalidMigrationRollback { detail } => {
                write!(formatter, "invalid migration rollback: {detail}")
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
            Self::InvalidDaemonEvent { detail } => {
                write!(formatter, "invalid daemon event: {detail}")
            }
            Self::InvalidDaemonOperation { detail } => {
                write!(formatter, "invalid daemon operation: {detail}")
            }
            Self::StateBackupIo {
                action,
                path,
                source,
            } => write!(
                formatter,
                "failed to {action} state backup '{}': {source}",
                path.display()
            ),
        }
    }
}

impl Error for StateStoreError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::StateBackupIo { source, .. } => Some(source),
            Self::UnsupportedSchema { .. }
            | Self::NonUtf8Path { .. }
            | Self::RouteOwnershipConflict { .. }
            | Self::InstallationAlreadyInitialized { .. }
            | Self::CredentialOwnershipConflict { .. }
            | Self::ResourceOwnershipConflict { .. }
            | Self::LogicalResourceOwnershipConflict { .. }
            | Self::InvalidLogicalEnvironment { .. }
            | Self::ResourceAdoptionRequired { .. }
            | Self::InvalidResourceRetirement { .. }
            | Self::InvalidLogicalResourceRetirement { .. }
            | Self::ProjectAdoptionRequired { .. }
            | Self::InvalidProjectAdoption { .. }
            | Self::MigrationIdentityConflict { .. }
            | Self::AcceptedV7InventoryConflict { .. }
            | Self::AcceptedV7ProjectIdentityConflict { .. }
            | Self::MigrationEvidenceConflict { .. }
            | Self::RecoveryPointEvidenceConflict { .. }
            | Self::InvalidMigrationTransition { .. }
            | Self::MigrationTimeRegression { .. }
            | Self::InvalidMigrationTarget { .. }
            | Self::MigrationTargetCredentialConflict { .. }
            | Self::InvalidMigrationCutover { .. }
            | Self::InvalidMigrationRollback { .. }
            | Self::ProjectAdoptionTargetMissing { .. }
            | Self::ProjectAdoptionStateMismatch { .. }
            | Self::CorruptState { .. }
            | Self::InvalidDaemonEvent { .. }
            | Self::InvalidDaemonOperation { .. } => None,
        }
    }
}

impl From<rusqlite::Error> for StateStoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}
