//! Strict v8 browser opening from authoritative published routes.

use anyhow::{Result, bail};

use crate::cli::args::{Cli, Commands, OpenArgs};
use crate::cli::browser_opener::try_open_in_browser;
use crate::cli::dispatch::context::CliDispatchContext;
use crate::control_plane::{IpcProjectStatus, IpcResourceHealth, IpcResourceLifecycle};

use super::v8_project::resolve_v8_project;
use super::v8_project_status::request_v8_project_status;
use super::v8_url_cmd::{render_routes, select_routes};

#[derive(Debug, Eq, PartialEq)]
enum V8OpenSelection<'service> {
    All,
    Service(&'service str),
}

pub(crate) fn handle_v8_open(cli: &Cli, context: &CliDispatchContext<'_>) -> Result<bool> {
    let Commands::Open(args) = &cli.command else {
        return Ok(false);
    };
    let Some(project) = resolve_v8_project(context)? else {
        return Ok(false);
    };
    let selection = open_selection(args)?;
    let selected_service = match selection {
        V8OpenSelection::All => None,
        V8OpenSelection::Service(service) => Some(service),
    };
    let status = request_v8_project_status(project.root())?;
    let routes = select_routes(&status, &project.service_names(), selected_service)?;
    if routes.is_empty() {
        bail!(
            "v8 project '{}' has no published HTTPS routes",
            status.project()
        );
    }
    ensure_routes_ready(&status, &routes)?;

    if args.json {
        render_routes(&mut std::io::stdout(), &routes, false, "json")?;
    } else if args.no_browser || context.non_interactive() {
        render_routes(
            &mut std::io::stdout(),
            &routes,
            selected_service.is_some(),
            "table",
        )?;
    } else {
        for (_, url) in routes {
            try_open_in_browser(&url)?;
        }
    }

    Ok(true)
}

pub(super) fn ensure_routes_ready(
    status: &IpcProjectStatus,
    routes: &[(String, String)],
) -> Result<()> {
    for (service, url) in routes {
        let resource = status
            .resources()
            .iter()
            .find(|resource| {
                resource.service() == service
                    && resource.lifecycle() == IpcResourceLifecycle::Active
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "v8 service '{service}' has no active reconciled runtime; wait for the daemon"
                )
            })?;
        match resource.health() {
            IpcResourceHealth::Healthy | IpcResourceHealth::RunningUnverified => {}
            health => bail!(
                "v8 service '{service}' is not ready (health: {}, observed_at: {}); wait for daemon reconciliation",
                health.as_str(),
                resource
                    .observed_at_unix_seconds()
                    .map_or_else(|| "never".to_owned(), |value| value.to_string())
            ),
        }
        let domain = url.strip_prefix("https://").ok_or_else(|| {
            anyhow::anyhow!("v8 route '{url}' is not an authoritative HTTPS route")
        })?;
        let route = status
            .resources()
            .iter()
            .find(|resource| {
                resource.service() == domain
                    && resource.kind() == "gateway_route"
                    && resource.lifecycle() == IpcResourceLifecycle::Active
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "v8 route '{domain}' has no active gateway observation; wait for the daemon"
                )
            })?;
        if route.health() != IpcResourceHealth::Healthy {
            bail!(
                "v8 route '{domain}' is not ready (health: {}, observed_at: {}); wait for daemon reconciliation",
                route.health().as_str(),
                route
                    .observed_at_unix_seconds()
                    .map_or_else(|| "never".to_owned(), |value| value.to_string())
            );
        }
    }

    Ok(())
}

fn open_selection(args: &OpenArgs) -> Result<V8OpenSelection<'_>> {
    if args.all {
        Ok(V8OpenSelection::All)
    } else {
        Ok(V8OpenSelection::Service(args.service().unwrap_or("app")))
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use crate::cli::args::{Cli, Commands};
    use crate::control_plane::{
        IpcProjectStatus, IpcResourceHealth, IpcResourceLifecycle, IpcResourceStatus,
    };

    use super::{V8OpenSelection, ensure_routes_ready, open_selection};

    #[test]
    fn open_defaults_to_the_exact_app_service() {
        let cli = Cli::parse_from(["stackctl", "open", "--no-browser"]);
        let Commands::Open(args) = cli.command else {
            panic!("open command");
        };

        assert_eq!(
            open_selection(&args).expect("default selection"),
            V8OpenSelection::Service("app")
        );
    }

    #[test]
    fn open_all_selects_every_published_route() {
        let cli = Cli::parse_from(["stackctl", "open", "--all", "--no-browser"]);
        let Commands::Open(args) = cli.command else {
            panic!("open command");
        };

        assert_eq!(
            open_selection(&args).expect("all selection"),
            V8OpenSelection::All
        );
    }

    #[test]
    fn parser_rejects_removed_database_and_health_probing_flags() {
        for arguments in [
            ["stackctl", "open", "--database"],
            ["stackctl", "open", "--health-path=/up"],
        ] {
            assert!(Cli::try_parse_from(arguments).is_err());
        }
    }

    #[test]
    fn open_requires_typed_ready_state_for_every_selected_route() {
        let routes = vec![(
            "app".to_owned(),
            "https://bill-app.stackctl.localhost".to_owned(),
        )];
        for (health, succeeds) in [
            (IpcResourceHealth::Healthy, true),
            (IpcResourceHealth::RunningUnverified, true),
            (IpcResourceHealth::Starting, false),
            (IpcResourceHealth::Unknown, false),
            (IpcResourceHealth::Unhealthy { failing_streak: 3 }, false),
        ] {
            let status = IpcProjectStatus::new(
                "bill".to_owned(),
                vec!["bill-app.stackctl.localhost".to_owned()],
                vec![
                    IpcResourceStatus::new(
                        "app".to_owned(),
                        "project_application".to_owned(),
                        IpcResourceLifecycle::Active,
                        health,
                        Some(10_000),
                        false,
                    ),
                    IpcResourceStatus::new(
                        "bill-app.stackctl.localhost".to_owned(),
                        "gateway_route".to_owned(),
                        IpcResourceLifecycle::Active,
                        IpcResourceHealth::Healthy,
                        Some(10_000),
                        true,
                    ),
                ],
            );

            assert_eq!(ensure_routes_ready(&status, &routes).is_ok(), succeeds);
        }

        let drift = IpcProjectStatus::new(
            "bill".to_owned(),
            vec!["bill-app.stackctl.localhost".to_owned()],
            vec![
                IpcResourceStatus::new(
                    "app".to_owned(),
                    "project_application".to_owned(),
                    IpcResourceLifecycle::Active,
                    IpcResourceHealth::Healthy,
                    Some(10_000),
                    false,
                ),
                IpcResourceStatus::new(
                    "bill-app.stackctl.localhost".to_owned(),
                    "gateway_route".to_owned(),
                    IpcResourceLifecycle::Active,
                    IpcResourceHealth::GatewayRouteDrift,
                    Some(10_001),
                    true,
                ),
            ],
        );

        assert!(ensure_routes_ready(&drift, &routes).is_err());
    }
}
