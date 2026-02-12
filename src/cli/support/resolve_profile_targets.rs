use anyhow::Result;

use crate::config;

pub(crate) fn resolve_profile_targets<'a>(
    config: &'a config::Config,
    profile: &str,
) -> Result<Vec<&'a config::ServiceConfig>> {
    let targets = match profile {
        "all" | "full" => config.service.iter().collect(),
        "infra" => config
            .service
            .iter()
            .filter(|svc| svc.kind != config::Kind::App)
            .collect(),
        "data" => config
            .service
            .iter()
            .filter(|svc| {
                matches!(
                    svc.kind,
                    config::Kind::Database
                        | config::Kind::Cache
                        | config::Kind::ObjectStore
                        | config::Kind::Search
                )
            })
            .collect(),
        "app" => config
            .service
            .iter()
            .filter(|svc| svc.kind == config::Kind::App)
            .collect(),
        "web" | "api" => {
            let primary_app = config::resolve_app_service(config, None)?;
            vec![primary_app]
        }
        _ => {
            anyhow::bail!(
                "unknown profile '{profile}'. expected one of: full, all, infra, data, app, web, api"
            )
        }
    };

    Ok(targets)
}
