use super::parse_project_config;
use crate::control_plane::resolve_desired_project;
use std::path::Path;

const CONFIG_PATH: &str = "/work/bill/.stackctl.yaml";

#[test]
fn parses_the_canonical_v8_service_mapping_without_coercion() {
    let source = r#"
schema_version: 8
project: bill
services:
  app:
    preset: laravel
    image: ghcr.io/stackctl/php:8.4
    php_extensions:
      - intl
      - redis
    depends_on:
      - db
  db:
    preset: postgres
    version: "17"
    database: bill
"#;

    let config = parse_project_config(source, Path::new(CONFIG_PATH)).expect("valid v8 config");

    assert_eq!(config.schema_version(), 8);
    assert_eq!(config.project(), Some("bill"));
    assert_eq!(config.services().len(), 2);
    assert_eq!(config.services()["db"].version(), Some("17"));
}

#[test]
fn desired_state_preserves_the_complete_service_declaration_deterministically() {
    let source = r#"
schema_version: 8
project: bill
services:
  app:
    preset: laravel
    image: ghcr.io/stackctl/php:8.4
    php_extensions: [redis, intl]
    depends_on: [db]
  db:
    preset: postgres
    version: "17"
    database: bill
"#;

    let desired = desired_from(source).expect("complete desired state");
    let app = desired.service("app").expect("app service");
    let database = desired.service("db").expect("database service");

    assert_eq!(app.preset(), Some("laravel"));
    assert_eq!(app.image(), Some("ghcr.io/stackctl/php:8.4"));
    assert_eq!(app.php_extensions(), ["intl", "redis"]);
    assert_eq!(app.version(), None);
    assert_eq!(database.preset(), Some("postgres"));
    assert_eq!(database.version(), Some("17"));
    assert_eq!(database.database(), Some("bill"));
}

#[test]
fn desired_state_rejects_empty_services_and_duplicate_extensions() {
    let empty = r#"
schema_version: 8
services:
  app: {}
"#;
    let duplicate_extension = r#"
schema_version: 8
services:
  app:
    preset: laravel
    php_extensions: [redis, redis]
"#;

    assert_eq!(
        desired_from(empty).expect_err("empty service").to_string(),
        "service 'app' must declare at least a preset or image"
    );
    assert_eq!(
        desired_from(duplicate_extension)
            .expect_err("duplicate extension")
            .to_string(),
        "service 'app' declares PHP extension 'redis' more than once"
    );
}

#[test]
fn rejects_duplicate_keys() {
    let source = "schema_version: 8\nproject: bill\nproject: bill-1\nservices: {}\n";

    let error = parse_project_config(source, Path::new(CONFIG_PATH)).expect_err("duplicate key");

    assert!(
        error.to_string().contains("duplicate field `project`"),
        "unexpected diagnostic: {error}"
    );
}

#[test]
fn rejects_unknown_fields_with_the_configuration_path() {
    let source = "schema_version: 8\nproject: bill\ncontainer_engine: docker\nservices: {}\n";

    let error = parse_project_config(source, Path::new(CONFIG_PATH)).expect_err("unknown field");

    assert!(error.to_string().contains(CONFIG_PATH));
    assert!(
        error
            .to_string()
            .contains("unknown field `container_engine`")
    );
}

#[test]
fn rejects_unknown_service_fields() {
    let source = r#"
schema_version: 8
services:
  app:
    preset: laravel
    privileged: true
"#;

    let error = parse_project_config(source, Path::new(CONFIG_PATH)).expect_err("unknown field");

    assert!(error.to_string().contains("privileged"));
}

#[test]
fn rejects_multiple_yaml_documents() {
    let source = "schema_version: 8\nservices: {}\n---\nschema_version: 8\nservices: {}\n";

    let error =
        parse_project_config(source, Path::new(CONFIG_PATH)).expect_err("multiple documents");

    assert!(error.to_string().contains("exactly one YAML document"));
}

#[test]
fn rejects_yaml_tags() {
    let source = "schema_version: 8\nproject: !stackctl bill\nservices: {}\n";

    let error = parse_project_config(source, Path::new(CONFIG_PATH)).expect_err("tagged value");

    assert!(error.to_string().contains("YAML tags are not supported"));
}

#[test]
fn rejects_numeric_service_versions() {
    let source = r#"
schema_version: 8
services:
  db:
    preset: postgres
    version: 17
"#;

    let error = parse_project_config(source, Path::new(CONFIG_PATH)).expect_err("numeric version");

    assert!(error.to_string().contains("services.db.version"));
    assert!(error.to_string().contains("must be a string"));
}

#[test]
fn rejects_unsupported_schema_versions_before_expansion() {
    let source = "schema_version: 7\nservices: {}\n";

    let error = parse_project_config(source, Path::new(CONFIG_PATH)).expect_err("old schema");

    assert!(
        error
            .to_string()
            .contains("schema_version 7 is unsupported; expected 8")
    );
}

#[test]
fn resolves_raw_configuration_into_exact_desired_identities() {
    let source = r#"
schema_version: 8
project: bill-1
services:
  app:
    preset: laravel
  mailpit:
    preset: mailpit
"#;
    let raw = parse_project_config(source, Path::new(CONFIG_PATH)).expect("valid raw config");

    let desired =
        resolve_desired_project(raw, Path::new("/work/ignored")).expect("valid desired project");

    assert_eq!(desired.project_name(), "bill-1");
    assert_eq!(desired.service_names(), ["app", "mailpit"]);
    assert_eq!(
        desired.route_domains(),
        [
            "bill-1-app.stackctl.localhost",
            "bill-1-mailpit.stackctl.localhost"
        ]
    );
}

#[test]
fn dependency_order_is_independent_of_yaml_map_order() {
    let first = r#"
schema_version: 8
services:
  app:
    preset: laravel
    depends_on: [db, cache]
  db:
    preset: postgres
  cache:
    preset: valkey
"#;
    let second = r#"
schema_version: 8
services:
  cache:
    preset: valkey
  db:
    preset: postgres
  app:
    preset: laravel
    depends_on: [cache, db]
"#;

    let first = desired_from(first).expect("first desired project");
    let second = desired_from(second).expect("second desired project");

    assert_eq!(first.startup_order(), second.startup_order());
    assert_eq!(first.startup_order(), ["cache", "db", "app"]);
}

#[test]
fn rejects_dependencies_that_are_not_declared_services() {
    let source = r#"
schema_version: 8
services:
  app:
    preset: laravel
    depends_on: [database]
"#;

    let error = desired_from(source).expect_err("unknown dependency");

    assert_eq!(
        error.to_string(),
        "service 'app' depends on unknown service 'database'"
    );
}

#[test]
fn rejects_dependency_cycles_with_the_complete_cycle() {
    let source = r#"
schema_version: 8
services:
  app:
    preset: laravel
    depends_on: [worker]
  worker:
    preset: worker
    depends_on: [app]
"#;

    let error = desired_from(source).expect_err("dependency cycle");

    assert_eq!(
        error.to_string(),
        "service dependency cycle: app -> worker -> app"
    );
}

#[test]
fn rejects_overlong_route_labels_during_desired_state_resolution() {
    let source = r#"
schema_version: 8
project: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
services:
  bbbbbbbbbbbbbbbbbbbbbbbb:
    preset: laravel
"#;

    let error = desired_from(source).expect_err("overlong route label");

    assert!(error.to_string().contains("exceeds 63 bytes"));
}

fn desired_from(
    source: &str,
) -> Result<crate::control_plane::DesiredProject, crate::control_plane::DesiredProjectError> {
    let raw = parse_project_config(source, Path::new(CONFIG_PATH)).expect("valid raw config");

    resolve_desired_project(raw, Path::new("/work/bill"))
}
