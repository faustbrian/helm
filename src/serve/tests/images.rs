use super::super::images::{
    derived_image_tag, normalize_php_extensions, render_derived_dockerfile,
    should_inject_frankenphp_server_name,
};
use super::super::sql_client_flavor::SqlClientFlavor;
use super::helpers::app_service;
use std::collections::HashMap;

#[test]
fn derived_image_tag_sanitizes_container_name() {
    let signature = "base=image;js=true;exts=pdo_mysql";
    assert!(
        derived_image_tag("acme/api:serve-app", signature)
            .starts_with("helm/acme-api-serve-app-serve-")
    );
}

#[test]
fn extension_dockerfile_contains_install_step() {
    let rendered = render_derived_dockerfile(
        "dunglas/frankenphp:php8.5",
        &["pdo_mysql".to_owned(), "intl".to_owned()],
        false,
        SqlClientFlavor::Mysql,
    );
    assert!(rendered.contains("FROM dunglas/frankenphp:php8.5"));
    assert!(rendered.contains("RUN install-php-extensions pdo_mysql intl"));
    assert!(rendered.contains("/usr/local/bin/mysqldump"));
}

#[test]
fn derived_dockerfile_can_include_js_tooling() {
    let rendered = render_derived_dockerfile(
        "dunglas/frankenphp:php8.5",
        &Vec::new(),
        true,
        SqlClientFlavor::Mysql,
    );
    assert!(rendered.contains("https://bun.sh/install"));
    assert!(rendered.contains("corepack enable"));
    assert!(rendered.contains("getcomposer.org/installer"));
    assert!(rendered.contains("default-mysql-client"));
    assert!(rendered.contains("/usr/local/bin/mysqldump"));
    assert!(rendered.contains("/usr/local/bin/mysql"));
    assert!(rendered.contains("--column-statistics=0|--set-gtid-purged=OFF"));
    assert!(rendered.contains("--ssl-mode=DISABLED"));
    assert!(rendered.contains("postgresql-client"));
    assert!(rendered.contains("memory_limit=2048M"));
}

#[test]
fn derived_dockerfile_uses_mariadb_client_when_requested() {
    let rendered = render_derived_dockerfile(
        "dunglas/frankenphp:php8.5",
        &Vec::new(),
        true,
        SqlClientFlavor::Mariadb,
    );
    assert!(rendered.contains("mariadb-client"));
    assert!(!rendered.contains("default-mysql-client"));
}

#[test]
fn normalize_php_extensions_maps_sqlite_to_sqlite3_and_dedupes() {
    let normalized = normalize_php_extensions(&[
        "sqlite".to_owned(),
        "pdo_mysql".to_owned(),
        "sqlite3".to_owned(),
    ]);

    assert_eq!(
        normalized,
        vec!["sqlite3".to_owned(), "pdo_mysql".to_owned()]
    );
}

#[test]
fn injects_server_name_for_frankenphp_http_targets() {
    let service = app_service();
    let injected = HashMap::new();
    assert!(should_inject_frankenphp_server_name(&service, &injected));
}

#[test]
fn does_not_inject_server_name_when_already_defined() {
    let service = app_service();
    let mut injected = HashMap::new();
    injected.insert("SERVER_NAME".to_owned(), ":80".to_owned());
    assert!(!should_inject_frankenphp_server_name(&service, &injected));
}
