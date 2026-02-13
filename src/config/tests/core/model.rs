use super::helpers::mysql_service;
use super::*;
use anyhow::Result;
#[test]
fn driver_serde_lowercase() {
    let toml = r#"
            [[service]]
            name = "test"
            kind = "database"
            driver = "postgres"
            image = "postgres:16"
            host = "localhost"
            port = 5432
            database = "testdb"
            username = "user"
            password = "pass"
        "#;

    let config: Config = toml::from_str(toml).expect("failed to parse");
    assert_eq!(config.service[0].driver, Driver::Postgres);
}

#[test]
fn container_name_format() {
    assert_eq!(
        mysql_service("mydb")
            .container_name()
            .expect("container name resolved"),
        "test-mydb"
    );
}

#[test]
fn default_port_postgres() {
    let mut svc = mysql_service("pg");
    svc.driver = Driver::Postgres;
    assert_eq!(svc.default_port(), 5432);
}

#[test]
fn default_port_mysql() {
    assert_eq!(mysql_service("my").default_port(), 3306);
}

#[test]
fn apply_runtime_env_appends_name_suffix_and_shifts_ports() -> Result<()> {
    let mut config = Config {
        schema_version: 1,
        container_prefix: Some("test".to_owned()),
        service: vec![mysql_service("db")],
        swarm: Vec::new(),
    };

    apply_runtime_env(&mut config, "test")?;

    assert_eq!(config.service[0].container_name()?, "test-db-testing");
    assert_eq!(config.service[0].port, 4306);
    Ok(())
}

#[test]
fn default_env_file_name_maps_test_to_testing_file() -> Result<()> {
    assert_eq!(default_env_file_name(None)?, ".env");
    assert_eq!(default_env_file_name(Some("local"))?, ".env");
    assert_eq!(default_env_file_name(Some("test"))?, ".env.testing");
    assert_eq!(default_env_file_name(Some("staging"))?, ".env.staging");
    Ok(())
}

#[test]
fn resolved_domains_prefers_primary_then_unique_aliases() {
    let mut app = mysql_service("app");
    app.kind = Kind::App;
    app.driver = Driver::Frankenphp;
    app.domain = Some("main.site".to_owned());
    app.domains = Some(vec![
        "main.site".to_owned(),
        "other.site".to_owned(),
        " OTHER.site ".to_owned(),
        "".to_owned(),
    ]);

    assert_eq!(app.resolved_domains(), vec!["main.site", "other.site"]);
    assert_eq!(app.primary_domain(), Some("main.site"));
}
