use super::{
    DiscoveryReconciliationError, DiscoveryReconciliationResult, ProjectDiscoveryIssue,
    ProjectDiscoveryOptions, discover_project_sources,
};
use crate::control_plane::application::{ControlPlane, RegistryPlanError, plan_project_registry};
use crate::control_plane::state::StateStore;
use crate::control_plane::{artifact_lock_required, parse_project_config};

/// Scans every authoritative root and publishes only a complete valid registry.
pub(crate) fn reconcile_watched_roots<Store>(
    control_plane: &mut ControlPlane<Store>,
    options: ProjectDiscoveryOptions,
    orphaned_at_unix_seconds: i64,
) -> Result<DiscoveryReconciliationResult, DiscoveryReconciliationError>
where
    Store: StateStore,
{
    let roots = control_plane.watched_roots()?;
    let report = discover_project_sources(&roots, options)?;
    if !report.issues().is_empty() {
        return Ok(DiscoveryReconciliationResult::blocked(report));
    }

    let registry = match plan_project_registry(report.sources()) {
        Ok(registry) => registry,
        Err(error) => {
            return Ok(DiscoveryReconciliationResult::blocked(
                report.with_issue(plan_issue(error)),
            ));
        }
    };
    let mut pending_locks = Vec::new();
    for source in report.sources() {
        if source.artifact_lock_path().is_some() {
            continue;
        }
        let config = match parse_project_config(source.yaml(), source.config_path()) {
            Ok(config) => config,
            Err(error) => {
                return Ok(DiscoveryReconciliationResult::blocked(report.with_issue(
                    ProjectDiscoveryIssue::InvalidConfiguration {
                        detail: error.to_string(),
                    },
                )));
            }
        };
        let required = match artifact_lock_required(&config) {
            Ok(required) => required,
            Err(detail) => {
                return Ok(DiscoveryReconciliationResult::blocked(report.with_issue(
                    ProjectDiscoveryIssue::InvalidConfiguration { detail },
                )));
            }
        };
        if required {
            pending_locks.push(source.canonical_path().join(".stackctl.lock.yaml"));
        }
    }
    if !pending_locks.is_empty() {
        let mut blocked = report;
        for path in pending_locks {
            blocked = blocked.with_issue(ProjectDiscoveryIssue::ArtifactLockPending { path });
        }
        return Ok(DiscoveryReconciliationResult::blocked(blocked));
    }

    control_plane.reconcile_discovered_registry(&registry, orphaned_at_unix_seconds)?;

    Ok(DiscoveryReconciliationResult::applied(report, registry))
}

fn plan_issue(error: RegistryPlanError) -> ProjectDiscoveryIssue {
    if error.is_ownership_collision() {
        ProjectDiscoveryIssue::ConfigurationCollision {
            detail: error.to_string(),
        }
    } else if error.is_security_policy_blocked() {
        ProjectDiscoveryIssue::SecurityApprovalBlocked {
            detail: error.to_string(),
        }
    } else {
        ProjectDiscoveryIssue::InvalidConfiguration {
            detail: error.to_string(),
        }
    }
}
