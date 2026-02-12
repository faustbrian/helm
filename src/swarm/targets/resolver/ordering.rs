use anyhow::Result;
use std::collections::{HashMap, HashSet};

pub(super) fn order_target_names(
    by_name: &HashMap<&str, &crate::config::SwarmTarget>,
    selected_roots: Vec<String>,
    include_deps: bool,
) -> Result<Vec<String>> {
    if !include_deps {
        return Ok(selected_roots);
    }

    let mut ordered = Vec::new();
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();

    for name in &selected_roots {
        visit(name, by_name, &mut visiting, &mut visited, &mut ordered)?;
    }

    Ok(ordered)
}

fn visit(
    current: &str,
    by_name: &HashMap<&str, &crate::config::SwarmTarget>,
    visiting: &mut HashSet<String>,
    visited: &mut HashSet<String>,
    ordered: &mut Vec<String>,
) -> Result<()> {
    if visited.contains(current) {
        return Ok(());
    }
    if !visiting.insert(current.to_owned()) {
        anyhow::bail!("circular swarm dependency detected at '{current}'");
    }

    let Some(target) = by_name.get(current).copied() else {
        anyhow::bail!("unknown swarm dependency target '{current}'");
    };
    for dependency in &target.depends_on {
        visit(dependency, by_name, visiting, visited, ordered)?;
    }

    visiting.remove(current);
    visited.insert(current.to_owned());
    ordered.push(current.to_owned());
    Ok(())
}
