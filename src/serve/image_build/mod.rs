//! Derived runtime image build helpers for serve targets.

use anyhow::Result;

use crate::config::ServiceConfig;
use crate::serve::sql_client_flavor::SqlClientFlavor;

mod build;
mod dockerfile;
mod inspect;

use inspect::installed_php_modules;

/// Filters extension list to packages not already installed in base image.
pub(super) fn filter_installable_extensions(
    base_image: &str,
    extensions: &[String],
) -> Result<Vec<String>> {
    let preinstalled = installed_php_modules(base_image)?;
    Ok(extensions
        .iter()
        .filter(|ext| !preinstalled.contains(&ext.to_lowercase()))
        .cloned()
        .collect())
}

/// Returns whether JS/composer tooling should be included in derived image.
pub(super) fn should_include_js_tooling(target: &ServiceConfig) -> bool {
    target.driver == crate::config::Driver::Frankenphp
}

/// Renders the Dockerfile used to build a derived serve runtime image.
pub(super) fn render_derived_dockerfile(
    base_image: &str,
    extensions: &[String],
    include_js_tooling: bool,
    sql_client_flavor: SqlClientFlavor,
) -> String {
    dockerfile::render_derived_dockerfile(
        base_image,
        extensions,
        include_js_tooling,
        sql_client_flavor,
    )
}

/// Builds a derived serve image from rendered Dockerfile content.
pub(super) fn build_derived_image(tag: &str, dockerfile: &str) -> Result<()> {
    build::build_derived_image(tag, dockerfile)
}
