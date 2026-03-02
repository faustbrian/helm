//! Shared service startup helper for CLI handlers.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::{config, docker, serve};

/// Immutable runtime context required to start services.
#[derive(Clone, Copy)]
pub(crate) struct ServiceStartContext<'a> {
    pub(crate) workspace_root: &'a Path,
    pub(crate) app_env: &'a HashMap<String, String>,
}

impl<'a> ServiceStartContext<'a> {
    pub(crate) fn new(workspace_root: &'a Path, app_env: &'a HashMap<String, String>) -> Self {
        Self {
            workspace_root,
            app_env,
        }
    }
}

/// Starts a service and optionally waits for health.
pub(crate) fn start_service(
    service: &config::ServiceConfig,
    context: &ServiceStartContext<'_>,
    recreate: bool,
    pull_policy: docker::PullPolicy,
    wait_healthy: bool,
    health_timeout_secs: u64,
    build: bool,
) -> Result<()> {
    if service.kind == config::Kind::App {
        serve::run(serve::RunServeOptions {
            target: service,
            recreate,
            trust_container_ca: service.trust_container_ca,
            detached: true,
            project_root: context.workspace_root,
            injected_env: context.app_env,
            allow_rebuild: build,
        })?;
        if wait_healthy {
            serve::wait_until_http_healthy(service, health_timeout_secs, 2, None)?;
        }
    } else {
        docker::up(service, pull_policy, recreate)?;
        if wait_healthy {
            docker::wait_until_healthy(service, health_timeout_secs, 2, None)?;
        }
    }

    Ok(())
}
