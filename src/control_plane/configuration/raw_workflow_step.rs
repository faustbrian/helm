use super::RawWorkflowMigration;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// One strict typed operation in a manually invoked workflow.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum RawWorkflowStep {
    DatabaseRestore {
        service: String,
        file: PathBuf,
        archive_entry: Option<String>,
        #[serde(default)]
        reset: bool,
        migrate: Option<RawWorkflowMigration>,
    },
    Open {
        service: String,
    },
}

impl RawWorkflowStep {
    pub(crate) fn service(&self) -> &str {
        match self {
            Self::DatabaseRestore { service, .. } | Self::Open { service } => service,
        }
    }

    pub(crate) fn file(&self) -> Option<&Path> {
        match self {
            Self::DatabaseRestore { file, .. } => Some(file),
            Self::Open { .. } => None,
        }
    }

    pub(crate) fn archive_entry(&self) -> Option<&str> {
        match self {
            Self::DatabaseRestore { archive_entry, .. } => archive_entry.as_deref(),
            Self::Open { .. } => None,
        }
    }

    pub(crate) const fn reset(&self) -> bool {
        match self {
            Self::DatabaseRestore { reset, .. } => *reset,
            Self::Open { .. } => false,
        }
    }

    pub(crate) fn migration_service(&self) -> Option<&str> {
        match self {
            Self::DatabaseRestore { migrate, .. } => {
                migrate.as_ref().map(RawWorkflowMigration::service)
            }
            Self::Open { .. } => None,
        }
    }

    pub(crate) fn migration_connection(&self) -> Option<&str> {
        match self {
            Self::DatabaseRestore { migrate, .. } => {
                migrate.as_ref().map(RawWorkflowMigration::connection)
            }
            Self::Open { .. } => None,
        }
    }
}
