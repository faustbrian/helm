//! Strict v8 route lookup from authoritative daemon state.

use std::collections::BTreeSet;
use std::io::{Write, stdout};
use std::path::Path;

use anyhow::{Result, bail};
use serde::Serialize;

use crate::cli::args::{Cli, Commands};
use crate::cli::dispatch::context::CliDispatchContext;
use crate::control_plane::{IpcProjectStatus, ProjectIdentity, RouteIdentity, ServiceIdentity};

use super::v8_project::resolve_v8_project;
use super::v8_project_status::request_v8_project_status;

#[derive(Serialize)]
struct PublishedRoute<'route> {
    name: &'route str,
    url: &'route str,
}

pub(crate) fn handle_v8_url(cli: &Cli, context: &CliDispatchContext<'_>) -> Result<bool> {
    let Commands::Url(args) = &cli.command else {
        return Ok(false);
    };
    let Some(project) = resolve_v8_project(context)? else {
        return Ok(false);
    };
    if !matches!(args.format.as_str(), "table" | "json") {
        bail!("v8 URL format must be 'table' or 'json'");
    }

    let status = request_v8_project_status(project.root())?;
    let routes = select_routes(&status, &project.service_names(), args.service())?;
    render_routes(
        &mut stdout(),
        &routes,
        args.service().is_some(),
        &args.format,
    )?;

    Ok(true)
}

pub(super) fn select_routes(
    status: &IpcProjectStatus,
    services: &[String],
    selected_service: Option<&str>,
) -> Result<Vec<(String, String)>> {
    if let Some(service) = selected_service
        && !services.iter().any(|candidate| candidate == service)
    {
        bail!("v8 service '{service}' is not declared in .stackctl.yaml");
    }

    let published = status
        .routes()
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let project = ProjectIdentity::resolve(Some(status.project()), Path::new("/"))?;
    let mut routes = Vec::new();
    for service in services {
        if selected_service.is_some_and(|selected| selected != service) {
            continue;
        }
        let identity = ServiceIdentity::new(service)?;
        let domain = RouteIdentity::new(&project, &identity)?;
        if published.contains(domain.domain()) {
            routes.push((service.clone(), format!("https://{}", domain.domain())));
        }
    }

    if let Some(service) = selected_service
        && routes.is_empty()
    {
        bail!("v8 service '{service}' has no published HTTPS route");
    }

    Ok(routes)
}

pub(super) fn render_routes(
    writer: &mut impl Write,
    routes: &[(String, String)],
    single_service: bool,
    format: &str,
) -> Result<()> {
    if format == "json" {
        let output = routes
            .iter()
            .map(|(name, url)| PublishedRoute { name, url })
            .collect::<Vec<_>>();
        serde_json::to_writer_pretty(&mut *writer, &output)?;
        writeln!(writer)?;
    } else if single_service {
        for (_, url) in routes {
            writeln!(writer, "{url}")?;
        }
    } else {
        for (name, url) in routes {
            writeln!(writer, "{name}: {url}")?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::control_plane::IpcProjectStatus;

    use super::{render_routes, select_routes};

    #[test]
    fn route_selection_uses_only_exact_published_service_domains() {
        let status = IpcProjectStatus::new(
            "bill".to_owned(),
            vec![
                "bill-mailpit.stackctl.localhost".to_owned(),
                "bill-app.stackctl.localhost".to_owned(),
            ],
            Vec::new(),
        );
        let services = ["app".to_owned(), "db".to_owned(), "mailpit".to_owned()];

        let routes = select_routes(&status, &services, None).expect("published routes");

        assert_eq!(
            routes,
            vec![
                (
                    "app".to_owned(),
                    "https://bill-app.stackctl.localhost".to_owned(),
                ),
                (
                    "mailpit".to_owned(),
                    "https://bill-mailpit.stackctl.localhost".to_owned(),
                ),
            ]
        );
    }

    #[test]
    fn exact_service_without_a_published_route_fails_loudly() {
        let status = IpcProjectStatus::new("bill".to_owned(), Vec::new(), Vec::new());
        let services = ["db".to_owned()];

        let error = select_routes(&status, &services, Some("db")).expect_err("missing route");

        assert!(error.to_string().contains("service 'db'"));
        assert!(error.to_string().contains("no published HTTPS route"));
    }

    #[test]
    fn route_output_preserves_existing_table_and_json_shapes() {
        let routes = vec![(
            "app".to_owned(),
            "https://bill-app.stackctl.localhost".to_owned(),
        )];
        let mut table = Vec::new();
        let mut single = Vec::new();
        let mut json = Vec::new();

        render_routes(&mut table, &routes, false, "table").expect("table routes");
        render_routes(&mut single, &routes, true, "table").expect("single route");
        render_routes(&mut json, &routes, false, "json").expect("json routes");

        assert_eq!(
            String::from_utf8(table).expect("table utf8"),
            "app: https://bill-app.stackctl.localhost\n"
        );
        assert_eq!(
            String::from_utf8(single).expect("single utf8"),
            "https://bill-app.stackctl.localhost\n"
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&json).expect("json output"),
            serde_json::json!([{
                "name": "app",
                "url": "https://bill-app.stackctl.localhost",
            }])
        );
    }
}
