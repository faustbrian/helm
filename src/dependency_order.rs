//! Shared dependency ordering helpers.

use anyhow::Result;
use std::collections::HashSet;

/// Topologically orders selected names by dependency relation.
pub(crate) fn order_dependency_names<FDep, FCycle>(
    selected_roots: &[String],
    dependencies_for: FDep,
    cycle_error: FCycle,
) -> Result<Vec<String>>
where
    FDep: Fn(&str) -> Result<Vec<String>>,
    FCycle: Fn(&str) -> String,
{
    let mut ordered = Vec::new();
    let mut visiting = HashSet::new();
    let mut visited = HashSet::new();

    fn visit<FDep, FCycle>(
        current: &str,
        dependencies_for: &FDep,
        cycle_error: &FCycle,
        visiting: &mut HashSet<String>,
        visited: &mut HashSet<String>,
        ordered: &mut Vec<String>,
    ) -> Result<()>
    where
        FDep: Fn(&str) -> Result<Vec<String>>,
        FCycle: Fn(&str) -> String,
    {
        if visited.contains(current) {
            return Ok(());
        }
        if !visiting.insert(current.to_owned()) {
            anyhow::bail!("{}", cycle_error(current));
        }

        for dependency in dependencies_for(current)? {
            visit(
                dependency.as_str(),
                dependencies_for,
                cycle_error,
                visiting,
                visited,
                ordered,
            )?;
        }

        visiting.remove(current);
        visited.insert(current.to_owned());
        ordered.push(current.to_owned());
        Ok(())
    }

    for name in selected_roots {
        visit(
            name,
            &dependencies_for,
            &cycle_error,
            &mut visiting,
            &mut visited,
            &mut ordered,
        )?;
    }

    Ok(ordered)
}
