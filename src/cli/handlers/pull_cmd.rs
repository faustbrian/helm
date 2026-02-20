//! cli handlers pull cmd module.
//!
//! Contains cli handlers pull cmd logic used by Helm command workflows.

use anyhow::Result;

use super::service_scope::for_each_service;
use crate::{config, docker};

pub(crate) fn handle_pull(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    parallel: usize,
) -> Result<()> {
    for_each_service(config, service, kind, parallel, docker::pull)
}
