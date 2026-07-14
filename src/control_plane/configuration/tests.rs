use super::{
    apply_artifact_lock, parse_artifact_lock, parse_project_config, project_config_schema,
};
use crate::control_plane::{
    KNOWN_SERVICE_PRESETS, PRESET_ARTIFACT_CATALOG_REVISION, resolve_desired_project,
};
use std::path::Path;

const CONFIG_PATH: &str = "/work/bill/.stackctl.yaml";
const LOCK_PATH: &str = "/work/bill/.stackctl.lock.yaml";

#[test]
fn artifact_lock_parser_rejects_ambiguous_or_extended_yaml() {
    let cases = [
        (
            "schema_version: 1\nimages: {}\nextra: true\n",
            "unknown field `extra`",
        ),
        (
            "schema_version: 1\nimages: {}\n---\nimages: {}\n",
            "expected exactly one YAML document",
        ),
        (
            "schema_version: 1\nimages: {}\nimages: {}\n",
            "duplicate field `images`",
        ),
    ];

    for (source, expected) in cases {
        let error = parse_artifact_lock(source, Path::new(LOCK_PATH)).expect_err(expected);

        assert!(error.to_string().contains(expected), "{error}");
        assert!(error.to_string().contains(LOCK_PATH));
    }
}

#[test]
fn artifact_lock_requires_immutable_resolutions_and_declared_services() {
    let mut config = parse_project_config(
        "schema_version: 8\nservices:\n  app:\n    image: ghcr.io/stackctl/php:8.4\n",
        Path::new(CONFIG_PATH),
    )
    .expect("project config");
    let mutable = parse_artifact_lock(
        concat!(
            "schema_version: 1\nimages:\n  app:\n",
            "    source: ghcr.io/stackctl/php:8.4\n",
            "    resolved: ghcr.io/stackctl/php:8.4\n"
        ),
        Path::new(LOCK_PATH),
    )
    .expect("syntactically valid lock");
    let unknown = parse_artifact_lock(
        concat!(
            "schema_version: 1\nimages:\n  worker:\n",
            "    source: ghcr.io/stackctl/php:8.4\n",
            "    resolved: ghcr.io/stackctl/php@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"
        ),
        Path::new(LOCK_PATH),
    )
    .expect("syntactically valid lock");

    assert!(
        apply_artifact_lock(&mut config, &mutable, Path::new(LOCK_PATH))
            .expect_err("mutable resolution")
            .to_string()
            .contains("must use an immutable sha256 digest")
    );
    assert!(
        apply_artifact_lock(&mut config, &unknown, Path::new(LOCK_PATH))
            .expect_err("unknown service")
            .to_string()
            .contains("does not match a declared service")
    );
}

#[test]
fn artifact_lock_source_for_a_versioned_preset_is_deterministic() {
    let mut config = parse_project_config(
        "schema_version: 8\nservices:\n  db:\n    preset: postgres\n    version: \"17\"\n",
        Path::new(CONFIG_PATH),
    )
    .expect("project config");
    let lock = parse_artifact_lock(
        concat!(
            "schema_version: 1\ncatalog_revision: 2026-07-14.1\nimages:\n  db:\n",
            "    source: preset:postgres:17\n",
            "    resolved: postgres@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"
        ),
        Path::new(LOCK_PATH),
    )
    .expect("artifact lock");

    apply_artifact_lock(&mut config, &lock, Path::new(LOCK_PATH)).expect("matching preset lock");

    assert_eq!(
        config.services()["db"].image(),
        Some(concat!(
            "postgres@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ))
    );
}

#[test]
fn artifact_lock_applies_the_catalog_default_version_for_an_omitted_preset_version() {
    let mut config = parse_project_config(
        "schema_version: 8\nservices:\n  mail:\n    preset: mailpit\n",
        Path::new(CONFIG_PATH),
    )
    .expect("project config");
    let lock = parse_artifact_lock(
        &format!(
            concat!(
                "schema_version: 1\ncatalog_revision: {}\nimages:\n  mail:\n",
                "    source: preset:mailpit\n",
                "    resolved: axllent/mailpit@sha256:",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"
            ),
            PRESET_ARTIFACT_CATALOG_REVISION
        ),
        Path::new(LOCK_PATH),
    )
    .expect("artifact lock");

    apply_artifact_lock(&mut config, &lock, Path::new(LOCK_PATH)).expect("matching preset lock");

    assert_eq!(config.services()["mail"].version(), Some("1"));
    assert_eq!(
        config.services()["mail"].image(),
        Some(concat!(
            "axllent/mailpit@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        ))
    );
}

#[test]
fn artifact_lock_rejects_a_stale_preset_catalog_revision() {
    let mut config = parse_project_config(
        "schema_version: 8\nservices:\n  db:\n    preset: postgres\n    version: \"17\"\n",
        Path::new(CONFIG_PATH),
    )
    .expect("project config");
    let lock = parse_artifact_lock(
        concat!(
            "schema_version: 1\ncatalog_revision: stale\nimages:\n  db:\n",
            "    source: preset:postgres:17\n",
            "    resolved: postgres@sha256:",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n"
        ),
        Path::new(LOCK_PATH),
    )
    .expect("artifact lock");

    let error = apply_artifact_lock(&mut config, &lock, Path::new(LOCK_PATH))
        .expect_err("stale catalog revision");

    assert!(error.to_string().contains("requires catalog_revision"));
    assert!(error.to_string().contains(PRESET_ARTIFACT_CATALOG_REVISION));
}

#[test]
fn exposes_a_versioned_editor_schema_matching_the_strict_yaml_shape() {
    let schema: serde_json::Value =
        serde_json::from_str(project_config_schema()).expect("valid bundled JSON Schema");

    assert_eq!(
        schema["$id"],
        "https://stackctl.dev/schemas/project/v8.json"
    );
    assert_eq!(schema["properties"]["schema_version"]["const"], 8);
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        schema["$defs"]["dnsLabel"]["pattern"],
        "^[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?$"
    );
    assert_eq!(
        schema["properties"]["services"]["propertyNames"]["$ref"],
        "#/$defs/dnsLabel"
    );
    assert_eq!(
        schema["$defs"]["service"]["properties"]["version"]["type"],
        "string"
    );
    assert_eq!(
        schema["$defs"]["service"]["properties"]["environment"]["propertyNames"]["pattern"],
        "^[A-Za-z_][A-Za-z0-9_]*$"
    );
    assert_eq!(
        schema["$defs"]["preset"]["enum"]
            .as_array()
            .expect("preset enum")
            .iter()
            .map(|value| value.as_str().expect("preset string"))
            .collect::<Vec<_>>(),
        KNOWN_SERVICE_PRESETS
    );
}

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
    command: [php, artisan, octane:start]
    environment:
      APP_ENV: local
      DB_PASSWORD: project-secret
      OCTANE_SERVER: frankenphp
    depends_on: [db]
  db:
    preset: postgres
    version: "17"
    database: bill
"#;

    let raw = parse_project_config(source, Path::new(CONFIG_PATH)).expect("raw desired state");
    assert!(!format!("{raw:?}").contains("project-secret"));

    let desired = desired_from(source).expect("complete desired state");
    let app = desired.service("app").expect("app service");
    let database = desired.service("db").expect("database service");

    assert_eq!(app.preset(), Some("laravel"));
    assert_eq!(app.image(), Some("ghcr.io/stackctl/php:8.4"));
    assert_eq!(app.php_extensions(), ["intl", "redis"]);
    assert_eq!(
        app.command().expect("application command"),
        ["php", "artisan", "octane:start"]
    );
    assert_eq!(
        app.environment(),
        &std::collections::BTreeMap::from([
            ("APP_ENV".to_owned(), "local".to_owned()),
            ("DB_PASSWORD".to_owned(), "project-secret".to_owned()),
            ("OCTANE_SERVER".to_owned(), "frankenphp".to_owned()),
        ])
    );
    assert!(!format!("{app:?}").contains("project-secret"));
    assert_eq!(app.version(), None);
    assert_eq!(database.preset(), Some("postgres"));
    assert_eq!(database.version(), Some("17"));
    assert_eq!(database.database(), Some("bill"));
}

#[test]
fn desired_state_rejects_unsafe_runtime_process_configuration() {
    let empty_command = r#"
schema_version: 8
services:
  worker:
    preset: queue-worker
    command: []
"#;
    let empty_executable = r#"
schema_version: 8
services:
  worker:
    preset: queue-worker
    command: [""]
"#;
    let invalid_environment_key = r#"
schema_version: 8
services:
  worker:
    preset: queue-worker
    environment:
      BAD=KEY: value
"#;

    assert_eq!(
        desired_from(empty_command)
            .expect_err("empty command")
            .to_string(),
        "service 'worker' command must contain a non-empty executable"
    );
    assert_eq!(
        desired_from(empty_executable)
            .expect_err("empty executable")
            .to_string(),
        "service 'worker' command must contain a non-empty executable"
    );
    assert_eq!(
        desired_from(invalid_environment_key)
            .expect_err("invalid environment key")
            .to_string(),
        "service 'worker' declares invalid environment key 'BAD=KEY'"
    );
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
fn desired_state_requires_an_extension_capable_application_preset() {
    let source = r#"
schema_version: 8
services:
  app:
    image: ghcr.io/acme/php@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
    php_extensions: [intl]
"#;

    assert_eq!(
        desired_from(source)
            .expect_err("custom extension installer contract")
            .to_string(),
        "service 'app' declares PHP extensions but preset '<none>' does not provide the pinned install-php-extensions runtime contract"
    );
}

#[test]
fn desired_state_rejects_presets_without_an_explicit_v8_strategy() {
    let source = r#"
schema_version: 8
services:
  app:
    preset: invented
"#;

    assert_eq!(
        desired_from(source)
            .expect_err("unknown preset")
            .to_string(),
        "service 'app' unknown v8 service preset 'invented'"
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
fn desired_routes_exclude_non_http_service_strategies() {
    let source = r#"
schema_version: 8
project: bill
services:
  app:
    preset: laravel
  db:
    preset: postgres
  cache:
    preset: valkey
  worker:
    preset: queue-worker
  scheduler:
    preset: scheduler
  mailpit:
    preset: mailpit
"#;

    let desired = desired_from(source).expect("valid desired project");

    assert_eq!(
        desired.route_domains(),
        [
            "bill-app.stackctl.localhost",
            "bill-mailpit.stackctl.localhost"
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
    preset: queue-worker
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
