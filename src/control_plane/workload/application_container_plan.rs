use super::{ApplicationContainerPlanOptions, WorkloadPlanError};
use crate::control_plane::gateway::GatewayRoute;
use crate::control_plane::{RouteIdentity, ServiceIdentity};
use std::path::{Path, PathBuf};

/// One dedicated project runtime reachable only through the private gateway.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ApplicationContainerPlan {
    project_id: String,
    container_name: String,
    image_digest: String,
    source_path: PathBuf,
    network_name: String,
    internal_http_port: u16,
    gateway_route: GatewayRoute,
}

impl ApplicationContainerPlan {
    pub(crate) fn new(options: ApplicationContainerPlanOptions) -> Result<Self, WorkloadPlanError> {
        validate_image_digest(&options.image_digest)?;

        if options.network_name.is_empty() {
            return Err(WorkloadPlanError::new(
                "application private network name must not be empty",
            ));
        }

        if !options.source_path.is_absolute() {
            return Err(WorkloadPlanError::new(format!(
                "application source path '{}' must be absolute",
                options.source_path.display()
            )));
        }

        if options.internal_http_port == 0 {
            return Err(WorkloadPlanError::new(
                "application internal HTTP port must be greater than zero",
            ));
        }

        let container_name = format!("stackctl-{}-app", options.project.as_str());
        let service = ServiceIdentity::new("app")
            .map_err(|error| WorkloadPlanError::new(error.to_string()))?;
        let route = RouteIdentity::new(&options.project, &service)
            .map_err(|error| WorkloadPlanError::new(error.to_string()))?;
        let gateway_route = GatewayRoute::new(
            route.domain(),
            format!("http://{container_name}:{}", options.internal_http_port),
        )
        .map_err(|error| WorkloadPlanError::new(error.to_string()))?;

        Ok(Self {
            project_id: options.project.as_str().to_owned(),
            container_name,
            image_digest: options.image_digest,
            source_path: options.source_path,
            network_name: options.network_name,
            internal_http_port: options.internal_http_port,
            gateway_route,
        })
    }

    pub(crate) fn container_name(&self) -> &str {
        &self.container_name
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.project_id
    }

    pub(crate) fn image_digest(&self) -> &str {
        &self.image_digest
    }

    pub(crate) fn source_path(&self) -> &Path {
        &self.source_path
    }

    pub(crate) fn network_name(&self) -> &str {
        &self.network_name
    }

    pub(crate) const fn internal_http_port(&self) -> u16 {
        self.internal_http_port
    }

    pub(crate) const fn published_ports(&self) -> &'static [u16] {
        &[]
    }

    pub(crate) const fn gateway_route(&self) -> &GatewayRoute {
        &self.gateway_route
    }
}

fn validate_image_digest(image: &str) -> Result<(), WorkloadPlanError> {
    let valid = image.rsplit_once("@sha256:").is_some_and(|(_, digest)| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    });

    if !valid {
        return Err(WorkloadPlanError::new(format!(
            "application image '{image}' must use an immutable sha256 digest"
        )));
    }

    Ok(())
}
