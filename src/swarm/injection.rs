use anyhow::Result;
use std::path::Path;

mod context;
mod resolver;
mod values;

#[cfg(test)]
pub(crate) use values::resolve_injected_env_value;

pub(crate) fn resolve_project_dependency_injected_env(
    project_root: &Path,
) -> Result<std::collections::HashMap<String, String>> {
    let Some(context) = resolve_workspace_swarm_context(project_root)? else {
        return Ok(std::collections::HashMap::new());
    };
    resolver::resolve_injected_env_from_swarm_context(&context)
}

pub(super) fn resolve_workspace_swarm_context(
    project_root: &Path,
) -> Result<Option<context::ProjectSwarmContext>> {
    context::resolve_workspace_swarm_context(project_root)
}
