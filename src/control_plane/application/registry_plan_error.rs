use crate::control_plane::configuration::{ArtifactLockError, ConfigParseError};
use crate::control_plane::{DesiredProjectError, RegistryConflicts};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;

/// A complete-registry parse, desired-state, or ownership failure.
#[derive(Debug)]
#[non_exhaustive]
pub(crate) enum RegistryPlanError {
    Configuration(ConfigParseError),
    ArtifactLock(ArtifactLockError),
    DesiredProject(DesiredProjectError),
    ProjectIdentityOwnership {
        conflicts: Vec<(String, Vec<PathBuf>)>,
    },
    RouteOwnership(RegistryConflicts),
}

impl Display for RegistryPlanError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Configuration(error) => Display::fmt(error, formatter),
            Self::ArtifactLock(error) => Display::fmt(error, formatter),
            Self::DesiredProject(error) => Display::fmt(error, formatter),
            Self::ProjectIdentityOwnership { conflicts } => {
                write!(
                    formatter,
                    "project registry contains conflicting identities:"
                )?;
                for (project, paths) in conflicts {
                    write!(formatter, "\n- project identity '{project}' is claimed by:")?;
                    for path in paths {
                        write!(formatter, "\n  - '{}'", path.display())?;
                    }
                }
                write!(
                    formatter,
                    "\nset a unique explicit 'project' value in each .stackctl.yaml or rename the directories"
                )
            }
            Self::RouteOwnership(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for RegistryPlanError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Configuration(error) => Some(error),
            Self::ArtifactLock(error) => Some(error),
            Self::DesiredProject(error) => Some(error),
            Self::ProjectIdentityOwnership { .. } => None,
            Self::RouteOwnership(error) => Some(error),
        }
    }
}

impl RegistryPlanError {
    pub(crate) const fn is_ownership_collision(&self) -> bool {
        matches!(
            self,
            Self::ProjectIdentityOwnership { .. } | Self::RouteOwnership(_)
        )
    }

    pub(crate) const fn is_security_policy_blocked(&self) -> bool {
        matches!(
            self,
            Self::Configuration(error) if error.is_security_policy_blocked()
        )
    }

    pub(crate) const fn is_artifact_lock_error(&self) -> bool {
        matches!(self, Self::ArtifactLock(_))
    }

    pub(crate) fn conflicting_paths(&self) -> Vec<PathBuf> {
        let mut paths = match self {
            Self::ProjectIdentityOwnership { conflicts } => conflicts
                .iter()
                .flat_map(|(_, paths)| paths.iter().cloned())
                .collect(),
            Self::RouteOwnership(conflicts) => conflicts
                .conflicts()
                .iter()
                .flat_map(|conflict| conflict.claims())
                .map(|claim| claim.canonical_project_path().to_path_buf())
                .collect(),
            _ => Vec::new(),
        };
        paths.sort();
        paths.dedup();

        paths
    }
}

impl From<ArtifactLockError> for RegistryPlanError {
    fn from(error: ArtifactLockError) -> Self {
        Self::ArtifactLock(error)
    }
}

impl From<ConfigParseError> for RegistryPlanError {
    fn from(error: ConfigParseError) -> Self {
        Self::Configuration(error)
    }
}

impl From<DesiredProjectError> for RegistryPlanError {
    fn from(error: DesiredProjectError) -> Self {
        Self::DesiredProject(error)
    }
}

impl From<RegistryConflicts> for RegistryPlanError {
    fn from(error: RegistryConflicts) -> Self {
        Self::RouteOwnership(error)
    }
}
