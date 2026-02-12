use anyhow::Result;

use crate::config;

use super::resolve_profile_targets::resolve_profile_targets;
use super::selected_services::selected_services;

pub(crate) fn resolve_up_services<'a>(
    config: &'a config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    profile: Option<&str>,
) -> Result<Vec<&'a config::ServiceConfig>> {
    let selected = if let Some(profile_name) = profile {
        resolve_profile_targets(config, profile_name)?
    } else {
        selected_services(config, service, kind, None)?
    };
    let mut ordered = Vec::new();
    let mut visiting = std::collections::HashSet::new();
    let mut visited = std::collections::HashSet::new();

    fn visit<'a>(
        config: &'a config::Config,
        svc: &'a config::ServiceConfig,
        visiting: &mut std::collections::HashSet<String>,
        visited: &mut std::collections::HashSet<String>,
        ordered: &mut Vec<&'a config::ServiceConfig>,
    ) -> Result<()> {
        if visited.contains(&svc.name) {
            return Ok(());
        }
        if !visiting.insert(svc.name.clone()) {
            anyhow::bail!("circular dependency detected at service '{}'", svc.name);
        }

        if let Some(depends_on) = &svc.depends_on {
            for dep in depends_on {
                let dep_svc = config::find_service(config, dep)?;
                visit(config, dep_svc, visiting, visited, ordered)?;
            }
        }

        visiting.remove(&svc.name);
        visited.insert(svc.name.clone());
        ordered.push(svc);
        Ok(())
    }

    for svc in selected {
        visit(config, svc, &mut visiting, &mut visited, &mut ordered)?;
    }

    Ok(ordered)
}
