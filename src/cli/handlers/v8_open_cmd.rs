//! Strict v8 browser opening from authoritative published routes.

use anyhow::{Result, bail};

use crate::cli::args::{Cli, Commands, OpenArgs};
use crate::cli::dispatch::context::CliDispatchContext;
use crate::cli::support::try_open_in_browser;
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

fn ensure_routes_ready(status: &IpcProjectStatus, routes: &[(String, String)]) -> Result<()> {
    for (service, _) in routes {
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
    }

    Ok(())
}

fn open_selection(args: &OpenArgs) -> Result<V8OpenSelection<'_>> {
    if args.kind.is_some() || args.profile().is_some() {
        bail!("v8 open requires an exact --service; --kind and --profile are not supported");
    }
    if args.database {
        bail!(
            "v8 open --database is not supported; export the daemon-owned managed environment explicitly"
        );
    }
    if args.health_path().is_some() {
        bail!(
            "v8 open --health-path is not supported; daemon health must come from typed readiness state"
        );
    }
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
    fn open_rejects_legacy_database_and_health_probing() {
        for arguments in [
            ["stackctl", "open", "--database"],
            ["stackctl", "open", "--health-path=/up"],
        ] {
            let cli = Cli::parse_from(arguments);
            let Commands::Open(args) = cli.command else {
                panic!("open command");
            };

            let error = open_selection(&args).expect_err("legacy open mode");

            assert!(error.to_string().contains("not supported"));
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
                vec![IpcResourceStatus::new(
                    "app".to_owned(),
                    "project_application".to_owned(),
                    IpcResourceLifecycle::Active,
                    health,
                    Some(10_000),
                    false,
                )],
            );

            assert_eq!(ensure_routes_ready(&status, &routes).is_ok(), succeeds);
        }
    }
}
