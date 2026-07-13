use super::{RegistryConflict, RegistryConflicts, RouteClaim, ValidatedRouteRegistry};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Validates the complete discovered registry before any caller can mutate it.
pub(crate) fn validate_route_claims(
    claims: Vec<RouteClaim>,
) -> Result<ValidatedRouteRegistry, RegistryConflicts> {
    let mut claims_by_domain = BTreeMap::<String, BTreeMap<PathBuf, RouteClaim>>::new();

    for claim in claims {
        claims_by_domain
            .entry(claim.domain().to_owned())
            .or_default()
            .entry(claim.canonical_project_path().to_path_buf())
            .or_insert(claim);
    }

    let mut accepted = Vec::new();
    let mut conflicts = Vec::new();

    for (domain, claims_by_path) in claims_by_domain {
        if claims_by_path.len() > 1 {
            conflicts.push(RegistryConflict::new(
                domain,
                claims_by_path.into_values().collect(),
            ));
        } else {
            accepted.extend(claims_by_path.into_values());
        }
    }

    if conflicts.is_empty() {
        Ok(ValidatedRouteRegistry::new(accepted))
    } else {
        Err(RegistryConflicts::new(conflicts))
    }
}
