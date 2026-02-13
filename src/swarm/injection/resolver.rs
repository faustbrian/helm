//! swarm injection resolver module.
//!
//! Contains swarm injection resolver logic used by Helm command workflows.

use anyhow::{Context, Result};

use super::super::targets::resolve_swarm_root;
use super::context::ProjectSwarmContext;
use super::values::resolve_injected_env_value;

/// Resolves injected env from swarm context using configured inputs and runtime state.
pub(super) fn resolve_injected_env_from_swarm_context(
    context: &ProjectSwarmContext,
) -> Result<std::collections::HashMap<String, String>> {
    if context.target.inject_env.is_empty() {
        return Ok(std::collections::HashMap::new());
    }

    let by_name: std::collections::HashMap<&str, &crate::config::SwarmTarget> = context
        .workspace_config
        .swarm
        .iter()
        .map(|target| (target.name.as_str(), target))
        .collect();

    let mut loaded_dependency_configs: std::collections::HashMap<String, crate::config::Config> =
        std::collections::HashMap::new();
    let mut result = std::collections::HashMap::new();

    for inject in &context.target.inject_env {
        let target_name = inject.from.trim();
        let Some(source_target) = by_name.get(target_name).copied() else {
            anyhow::bail!(
                "workspace target '{}' injects env from unknown target '{}'",
                context.target.name,
                inject.from
            );
        };

        if !loaded_dependency_configs.contains_key(target_name) {
            let dependency_root = resolve_swarm_root(&context.workspace_root, &source_target.root);
            let dependency_config_path = dependency_root.join(".helm.toml");
            let dependency_config =
                crate::config::load_config_with(Some(&dependency_config_path), None).with_context(
                    || {
                        format!(
                            "failed to load dependency config for target '{}' from {}",
                            target_name,
                            dependency_config_path.display()
                        )
                    },
                )?;
            loaded_dependency_configs.insert(target_name.to_owned(), dependency_config);
        }

        let dependency_config = loaded_dependency_configs.get(target_name).ok_or_else(|| {
            anyhow::anyhow!(
                "internal error: dependency config cache missing target '{}'",
                target_name
            )
        })?;
        let source_service =
            crate::config::resolve_app_service(dependency_config, inject.service.as_deref())
                .with_context(|| {
                    format!(
                        "failed resolving app service for inject env '{}' from target '{}'",
                        inject.env, target_name
                    )
                })?;
        let runtime_host_port = source_service
            .container_name()
            .ok()
            .and_then(|container_name| {
                crate::docker::inspect_host_port_binding(
                    &container_name,
                    source_service.resolved_container_port(),
                )
                .map(|(_, host_port)| host_port)
            });
        let resolved_value =
            resolve_injected_env_value(inject.value.trim(), source_service, runtime_host_port)
                .with_context(|| {
                    format!(
                        "failed resolving inject value '{}' for env '{}' from target '{}'",
                        inject.value, inject.env, target_name
                    )
                })?;

        result.insert(inject.env.trim().to_owned(), resolved_value);
    }

    Ok(result)
}
