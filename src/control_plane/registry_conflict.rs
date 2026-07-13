use super::RouteClaim;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Every distinct canonical path claiming one route domain.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RegistryConflict {
    domain: String,
    claims: Vec<RouteClaim>,
}

impl RegistryConflict {
    pub(super) fn new(domain: String, claims: Vec<RouteClaim>) -> Self {
        Self { domain, claims }
    }

    /// Returns the route domain with conflicting ownership.
    pub(crate) fn domain(&self) -> &str {
        &self.domain
    }

    pub(super) fn claims(&self) -> &[RouteClaim] {
        &self.claims
    }
}

/// All ownership conflicts found while validating a complete registry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RegistryConflicts {
    conflicts: Vec<RegistryConflict>,
}

impl RegistryConflicts {
    pub(super) fn new(conflicts: Vec<RegistryConflict>) -> Self {
        Self { conflicts }
    }

    /// Returns every conflict in deterministic domain order.
    pub(crate) fn conflicts(&self) -> &[RegistryConflict] {
        &self.conflicts
    }
}

impl Display for RegistryConflicts {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "route registry contains conflicting ownership:")?;

        for conflict in &self.conflicts {
            write!(formatter, "\n- {} is claimed by:", conflict.domain())?;

            for claim in conflict.claims() {
                write!(
                    formatter,
                    "\n  - project '{}', service '{}', path '{}'",
                    claim.project_name(),
                    claim.service_name(),
                    claim.canonical_project_path().display()
                )?;
            }
        }

        Ok(())
    }
}

impl Error for RegistryConflicts {}
