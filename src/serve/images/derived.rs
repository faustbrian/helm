//! Derived runtime image planning/build pipeline.
//!
//! Selects when to use base image, cached derived image, or a newly built image
//! based on extension/tooling requirements and rebuild policy.

use anyhow::Result;
use std::collections::HashMap;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};
use crate::serve::sql_client_flavor::sql_client_flavor_from_injected_env;

use super::super::image_build::{
    build_derived_image, filter_installable_extensions, render_derived_dockerfile,
    should_include_js_tooling,
};
use super::lock::{docker_image_exists, read_derived_image_lock, write_derived_image_lock};
use super::runtime::normalize_php_extensions;
use signature::derive_image_signature;

mod signature;

/// Resolves the runtime image tag for this serve target.
///
/// Prefers cached derived images by signature when available; falls back to base
/// image when no derived requirements exist or rebuild is disallowed.
pub(super) fn resolve_runtime_image(
    target: &ServiceConfig,
    allow_rebuild: bool,
    injected_env: &HashMap<String, String>,
) -> Result<String> {
    let include_js_tooling = should_include_js_tooling(target);
    let sql_client_flavor = sql_client_flavor_from_injected_env(injected_env);
    let normalized_extensions = target
        .php_extensions
        .as_ref()
        .filter(|exts| !exts.is_empty())
        .map(|exts| normalize_php_extensions(exts))
        .unwrap_or_default();

    if normalized_extensions.is_empty() && !include_js_tooling {
        return Ok(target.image.clone());
    }

    let installable_extensions =
        filter_installable_extensions(&target.image, &normalized_extensions)?;
    if installable_extensions.is_empty() && !include_js_tooling {
        return Ok(target.image.clone());
    }

    let container_name = target.container_name()?;
    let dockerfile = render_derived_dockerfile(
        &target.image,
        &installable_extensions,
        include_js_tooling,
        sql_client_flavor,
    );
    let signature = derive_image_signature(&dockerfile);
    if let Some(tag) = read_derived_image_lock()?.entries.get(&signature).cloned()
        && docker_image_exists(&tag)?
    {
        emit_derived_event(
            target,
            LogLevel::Info,
            Persistence::Persistent,
            &format!("Using cached derived image {tag}"),
        );
        return Ok(tag);
    }
    if !allow_rebuild {
        emit_derived_event(
            target,
            LogLevel::Warn,
            Persistence::Persistent,
            &format!(
                "Skipped rebuilding derived image because base image is used: {}",
                target.image
            ),
        );
        return Ok(target.image.clone());
    }

    let derived_tag = derived_image_tag(&container_name, &signature);
    if crate::docker::is_dry_run() {
        emit_derived_event(
            target,
            LogLevel::Info,
            Persistence::Transient,
            &format!(
                "[dry-run] Build derived image {derived_tag} with extensions: {}",
                installable_extensions.join(", ")
            ),
        );
        return Ok(derived_tag);
    }

    build_derived_image(&derived_tag, &dockerfile)?;
    let mut lock = read_derived_image_lock()?;
    lock.entries.insert(signature, derived_tag.clone());
    write_derived_image_lock(&lock)?;
    emit_derived_event(
        target,
        LogLevel::Success,
        Persistence::Persistent,
        &format!(
            "Prepared derived image {derived_tag} with extensions: {}",
            installable_extensions.join(", ")
        ),
    );
    Ok(derived_tag)
}

/// Builds a stable derived image tag from container identity and content signature.
pub(super) fn derived_image_tag(container_name: &str, signature: &str) -> String {
    signature::derived_image_tag(container_name, signature)
}

fn emit_derived_event(
    target: &ServiceConfig,
    level: LogLevel,
    persistence: Persistence,
    message: &str,
) {
    output::event(&target.name, level, message, persistence);
}
