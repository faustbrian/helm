use anyhow::Result;
use std::collections::HashMap;

use crate::config::ServiceConfig;

mod derived;
mod lock;
mod runtime;

pub(super) fn resolve_runtime_image(
    target: &ServiceConfig,
    allow_rebuild: bool,
    injected_env: &HashMap<String, String>,
) -> Result<String> {
    derived::resolve_runtime_image(target, allow_rebuild, injected_env)
}

pub(super) fn normalize_php_extensions(extensions: &[String]) -> Vec<String> {
    runtime::normalize_php_extensions(extensions)
}

pub(super) fn should_inject_frankenphp_server_name(
    target: &ServiceConfig,
    injected_env: &HashMap<String, String>,
) -> bool {
    runtime::should_inject_frankenphp_server_name(target, injected_env)
}

pub(super) fn mailhog_smtp_port(target: &ServiceConfig) -> Option<u16> {
    runtime::mailhog_smtp_port(target)
}

#[cfg(test)]
pub(super) fn derived_image_tag(container_name: &str, signature: &str) -> String {
    derived::derived_image_tag(container_name, signature)
}

#[cfg(test)]
pub(super) use super::image_build::render_derived_dockerfile;
