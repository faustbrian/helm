use anyhow::Result;

use crate::config::ServiceConfig;
use crate::serve::sql_client_flavor::SqlClientFlavor;

mod build;
mod dockerfile;
mod inspect;

use inspect::installed_php_modules;

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

pub(super) fn should_include_js_tooling(target: &ServiceConfig) -> bool {
    target.driver == crate::config::Driver::Frankenphp
}

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

pub(super) fn build_derived_image(tag: &str, dockerfile: &str) -> Result<()> {
    build::build_derived_image(tag, dockerfile)
}
