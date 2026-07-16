//! Strict v8 browser opening from authoritative published routes.

use std::time::Duration;

use anyhow::{Result, anyhow, bail};

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

const OPEN_READINESS_ATTEMPTS: usize = 600;
const OPEN_READINESS_INTERVAL: Duration = Duration::from_secs(1);

struct RouteReadinessError {
    message: String,
    retryable: bool,
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
    if route_readiness(&status, &routes).is_err() {
        drop(wait_for_routes_ready(
            || request_v8_project_status(project.root()),
            &routes,
            OPEN_READINESS_ATTEMPTS,
            OPEN_READINESS_INTERVAL,
        )?);
    }

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
    route_readiness(status, routes).map_err(|error| anyhow!(error.message))
}

fn route_readiness(
    status: &IpcProjectStatus,
    routes: &[(String, String)],
) -> std::result::Result<(), RouteReadinessError> {
    for (service, url) in routes {
        let resource = status
            .resources()
            .iter()
            .find(|resource| {
                resource.service() == service
                    && resource.lifecycle() == IpcResourceLifecycle::Active
            })
            .ok_or_else(|| {
                if let Some(retained) = status
                    .resources()
                    .iter()
                    .find(|resource| resource.service() == service)
                {
                    return RouteReadinessError {
                        message: format!(
                            "v8 service '{service}' is {} and automatic reactivation did not complete; `stackctl daemon service status` reports the terminal cause",
                            retained.lifecycle().as_str()
                        ),
                        retryable: false,
                    };
                }
                RouteReadinessError {
                    message: format!(
                        "v8 service '{service}' has no active reconciled runtime"
                    ),
                    retryable: true,
                }
            })?;
        match resource.health() {
            IpcResourceHealth::Healthy | IpcResourceHealth::RunningUnverified => {}
            health => {
                return Err(RouteReadinessError {
                    message: format!(
                        "v8 service '{service}' is not ready (health: {}, observed_at: {})",
                        health.as_str(),
                        resource
                            .observed_at_unix_seconds()
                            .map_or_else(|| "never".to_owned(), |value| value.to_string())
                    ),
                    retryable: !matches!(
                        health,
                        IpcResourceHealth::LogicalResourceDrift
                            | IpcResourceHealth::DestructiveReplacementRequired
                    ),
                });
            }
        }
        let domain = url
            .strip_prefix("https://")
            .ok_or_else(|| RouteReadinessError {
                message: format!("v8 route '{url}' is not an authoritative HTTPS route"),
                retryable: false,
            })?;
        let route = status
            .resources()
            .iter()
            .find(|resource| {
                resource.service() == domain
                    && resource.kind() == "gateway_route"
                    && resource.lifecycle() == IpcResourceLifecycle::Active
            })
            .ok_or_else(|| RouteReadinessError {
                message: format!("v8 route '{domain}' has no active gateway observation"),
                retryable: true,
            })?;
        if route.health() != IpcResourceHealth::Healthy {
            return Err(RouteReadinessError {
                message: format!(
                    "v8 route '{domain}' is not ready (health: {}, observed_at: {})",
                    route.health().as_str(),
                    route
                        .observed_at_unix_seconds()
                        .map_or_else(|| "never".to_owned(), |value| value.to_string())
                ),
                retryable: !matches!(
                    route.health(),
                    IpcResourceHealth::LogicalResourceDrift
                        | IpcResourceHealth::DestructiveReplacementRequired
                ),
            });
        }
    }

    Ok(())
}

fn wait_for_routes_ready(
    mut status: impl FnMut() -> Result<IpcProjectStatus>,
    routes: &[(String, String)],
    attempts: usize,
    interval: Duration,
) -> Result<IpcProjectStatus> {
    if attempts == 0 {
        bail!("open readiness attempts must be greater than zero");
    }
    let mut last_error = None;
    for attempt in 0..attempts {
        let current = status()?;
        match route_readiness(&current, routes) {
            Ok(()) => return Ok(current),
            Err(error) if !error.retryable => return Err(anyhow!(error.message)),
            Err(error) => last_error = Some(error.message),
        }
        if attempt + 1 < attempts && !interval.is_zero() {
            std::thread::sleep(interval);
        }
    }

    bail!(
        "v8 routes did not become ready after {attempts} checks: {}",
        last_error.unwrap_or_else(|| "no readiness observation was returned".to_owned())
    )
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

    use super::{V8OpenSelection, ensure_routes_ready, open_selection, wait_for_routes_ready};

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

    #[test]
    fn open_explains_failed_automatic_reactivation() {
        let routes = vec![(
            "app".to_owned(),
            "https://bill-app.stackctl.localhost".to_owned(),
        )];
        let status = IpcProjectStatus::new(
            "bill".to_owned(),
            vec!["bill-app.stackctl.localhost".to_owned()],
            vec![IpcResourceStatus::new(
                "app".to_owned(),
                "project_application".to_owned(),
                IpcResourceLifecycle::Retained,
                IpcResourceHealth::Unknown,
                None,
                false,
            )],
        );

        let error = ensure_routes_ready(&status, &routes).expect_err("retained runtime");

        assert!(error.to_string().contains("retained"));
        assert!(error.to_string().contains("automatic reactivation"));
        assert!(error.to_string().contains("daemon service status"));
    }

    #[test]
    fn open_waits_for_project_reconciliation_instead_of_requiring_a_rerun() {
        let routes = vec![(
            "app".to_owned(),
            "https://bill-app.stackctl.localhost".to_owned(),
        )];
        let mut attempts = 0;

        let status = wait_for_routes_ready(
            || {
                attempts += 1;
                Ok(project_status(if attempts == 1 {
                    IpcResourceHealth::Starting
                } else {
                    IpcResourceHealth::Healthy
                }))
            },
            &routes,
            3,
            std::time::Duration::ZERO,
        )
        .expect("eventual project readiness");

        assert_eq!(attempts, 2);
        assert_eq!(status.project(), "bill");
    }

    #[test]
    fn open_does_not_wait_on_terminal_reactivation_failure() {
        let routes = vec![(
            "app".to_owned(),
            "https://bill-app.stackctl.localhost".to_owned(),
        )];
        let retained = IpcProjectStatus::new(
            "bill".to_owned(),
            vec!["bill-app.stackctl.localhost".to_owned()],
            vec![IpcResourceStatus::new(
                "app".to_owned(),
                "project_application".to_owned(),
                IpcResourceLifecycle::Retained,
                IpcResourceHealth::Unknown,
                None,
                false,
            )],
        );
        let mut attempts = 0;

        let error = wait_for_routes_ready(
            || {
                attempts += 1;
                Ok(retained.clone())
            },
            &routes,
            10,
            std::time::Duration::ZERO,
        )
        .expect_err("terminal retained runtime");

        assert_eq!(attempts, 1);
        assert!(error.to_string().contains("automatic reactivation"));
    }

    fn project_status(health: IpcResourceHealth) -> IpcProjectStatus {
        IpcProjectStatus::new(
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
        )
    }
}
