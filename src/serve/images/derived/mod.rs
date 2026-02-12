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
    let signature = derive_image_signature(
        &target.image,
        include_js_tooling,
        sql_client_flavor.as_str(),
        &installable_extensions,
    );
    if let Some(tag) = read_derived_image_lock()?.entries.get(&signature).cloned()
        && docker_image_exists(&tag)?
    {
        output::event(
            &target.name,
            LogLevel::Info,
            &format!("Using cached derived image {tag}"),
            Persistence::Persistent,
        );
        return Ok(tag);
    }
    if !allow_rebuild {
        output::event(
            &target.name,
            LogLevel::Warn,
            &format!(
                "Skipped rebuilding derived image because base image is used: {}",
                target.image
            ),
            Persistence::Persistent,
        );
        return Ok(target.image.clone());
    }

    let derived_tag = derived_image_tag(&container_name, &signature);
    let dockerfile = render_derived_dockerfile(
        &target.image,
        &installable_extensions,
        include_js_tooling,
        sql_client_flavor,
    );

    if crate::docker::is_dry_run() {
        output::event(
            &target.name,
            LogLevel::Info,
            &format!(
                "[dry-run] Build derived image {derived_tag} with extensions: {}",
                installable_extensions.join(", ")
            ),
            Persistence::Transient,
        );
        return Ok(derived_tag);
    }

    build_derived_image(&derived_tag, &dockerfile)?;
    let mut lock = read_derived_image_lock()?;
    lock.entries.insert(signature, derived_tag.clone());
    write_derived_image_lock(&lock)?;
    output::event(
        &target.name,
        LogLevel::Success,
        &format!(
            "Prepared derived image {derived_tag} with extensions: {}",
            installable_extensions.join(", ")
        ),
        Persistence::Persistent,
    );
    Ok(derived_tag)
}

pub(super) fn derived_image_tag(container_name: &str, signature: &str) -> String {
    signature::derived_image_tag(container_name, signature)
}
