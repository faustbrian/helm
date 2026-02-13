//! config validation container names module.
//!
//! Contains config validation container names logic used by Helm command workflows.

use anyhow::Result;
use std::collections::HashSet;

use crate::config::Config;

/// Validates and resolves container names and reports actionable failures.
pub(super) fn validate_and_resolve_container_names(config: &mut Config) -> Result<()> {
    let prefix = config
        .container_prefix
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    if config.service.is_empty() {
        return Ok(());
    }

    let mut missing = Vec::new();
    let mut resolved_names = HashSet::new();

    for service in &mut config.service {
        let explicit = service
            .container_name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);

        let resolved = if let Some(explicit_name) = explicit {
            explicit_name
        } else if let Some(prefix_value) = &prefix {
            format!("{prefix_value}-{}", service.name)
        } else {
            missing.push(service.name.clone());
            continue;
        };

        if !resolved_names.insert(resolved.clone()) {
            anyhow::bail!("duplicate container name resolved: '{resolved}'");
        }

        service.resolved_container_name = Some(resolved);
    }

    if !missing.is_empty() {
        anyhow::bail!(
            "missing container naming strategy: set `container_prefix` or \
             `container_name` on each service (missing: {})",
            missing.join(", ")
        );
    }

    Ok(())
}
