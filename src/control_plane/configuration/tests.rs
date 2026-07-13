use super::parse_project_config;
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
