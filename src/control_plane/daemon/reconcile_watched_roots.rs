use super::{
    DiscoveryReconciliationError, DiscoveryReconciliationResult, ProjectDiscoveryIssue,
    ProjectDiscoveryOptions, discover_project_sources,
};
use crate::control_plane::application::{ControlPlane, RegistryPlanError, plan_project_registry};
use crate::control_plane::state::StateStore;
use crate::control_plane::{
    apply_artifact_lock, artifact_lock_required, parse_artifact_lock, parse_project_config,
};

/// Scans every authoritative root and publishes every independently valid project.
pub(crate) fn reconcile_watched_roots<Store>(
    control_plane: &mut ControlPlane<Store>,
    options: ProjectDiscoveryOptions,
    orphaned_at_unix_seconds: i64,
) -> Result<DiscoveryReconciliationResult, DiscoveryReconciliationError>
where
    Store: StateStore,
{
    let roots = control_plane.watched_roots()?;
    let discovered = discover_project_sources(&roots, options)?;
    if !discovered.issues().is_empty() {
        return Ok(DiscoveryReconciliationResult::blocked(discovered));
    }

    let mut issues = Vec::new();
    let mut parsed_sources = Vec::new();
    for source in discovered.sources() {
        let config = match parse_project_config(source.yaml(), source.config_path()) {
            Ok(config) => config,
            Err(error) => {
                let detail = error.to_string();
                issues.push(if error.is_security_policy_blocked() {
                    ProjectDiscoveryIssue::SecurityApprovalBlocked { detail }
                } else {
                    ProjectDiscoveryIssue::InvalidConfiguration { detail }
                });
                continue;
            }
        };
        let requires_artifact_lock = match artifact_lock_required(&config) {
            Ok(required) => required,
            Err(detail) => {
                issues.push(ProjectDiscoveryIssue::InvalidConfiguration { detail });
                continue;
            }
        };
        match plan_project_registry(std::slice::from_ref(source)) {
            Ok(_) => {}
            Err(error) if error.is_artifact_lock_error() => {}
            Err(error) => {
                issues.push(plan_issue(error));
                continue;
            }
        }
        parsed_sources.push((source.clone(), config, requires_artifact_lock));
    }

    loop {
        let unlocked = parsed_sources
            .iter()
            .map(|(source, _, _)| {
                crate::control_plane::application::ProjectSource::new(
                    source.canonical_path().to_path_buf(),
                    source.config_path().to_path_buf(),
                    source.yaml().to_owned(),
                )
            })
            .collect::<Vec<_>>();
        let Err(error) = plan_project_registry(&unlocked) else {
            break;
        };
        if !error.is_ownership_collision() {
            break;
        }
        let conflicting_paths = error
            .conflicting_paths()
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        issues.push(plan_issue(error));
        parsed_sources
            .retain(|(source, _, _)| !conflicting_paths.contains(source.canonical_path()));
    }

    let mut pending_locks = Vec::new();
    let mut valid_sources = Vec::new();
    for (source, config, requires_artifact_lock) in parsed_sources {
        if requires_artifact_lock {
            let lock_path = source.artifact_lock_path().map_or_else(
                || source.canonical_path().join(".stackctl.lock.yaml"),
                std::path::Path::to_path_buf,
            );
            let lock_needs_materialization = match source.artifact_lock_yaml() {
                None => true,
                Some(yaml) => parse_artifact_lock(yaml, &lock_path)
                    .ok()
                    .is_some_and(|lock| {
                        let mut resolved = config.clone();
                        apply_artifact_lock(&mut resolved, &lock, &lock_path).is_err()
                    }),
            };
            if lock_needs_materialization {
                pending_locks.push(lock_path);
            }
        }
        valid_sources.push(source);
    }
    let mut report = super::ProjectDiscoveryReport::new(valid_sources, issues);
    if !pending_locks.is_empty() {
        for path in pending_locks {
            report = report.with_issue(ProjectDiscoveryIssue::ArtifactLockPending { path });
        }
        return Ok(DiscoveryReconciliationResult::blocked(report));
    }

    let mut candidates = Vec::new();
    let individually_validated = report.sources().to_vec();
    for source in individually_validated {
        match plan_project_registry(std::slice::from_ref(&source)) {
            Ok(_) => candidates.push(source),
            Err(error) => report = report.with_issue(plan_issue(error)),
        }
    }
    let registry = loop {
        match plan_project_registry(&candidates) {
            Ok(registry) => break registry,
            Err(error) if error.is_ownership_collision() => {
                let conflicting_paths = error
                    .conflicting_paths()
                    .into_iter()
                    .collect::<std::collections::BTreeSet<_>>();
                report = report.with_issue(plan_issue(error));
                candidates.retain(|source| !conflicting_paths.contains(source.canonical_path()));
            }
            Err(error) => {
                return Ok(DiscoveryReconciliationResult::blocked(
                    report.with_issue(plan_issue(error)),
                ));
            }
        }
    };

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
