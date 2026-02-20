use super::*;
#[test]
fn parse_app_service_section() {
    let toml = r#"
            container_prefix = "app"

            [[service]]
            name = "db"
            kind = "database"
            driver = "mysql"
            image = "mysql:8.0"
            host = "127.0.0.1"
            port = 3306
            database = "app"
            username = "root"
            password = "secret"

            [[service]]
            name = "web"
            kind = "app"
            driver = "frankenphp"
            image = "dunglas/frankenphp:php8.5"
            host = "127.0.0.1"
            port = 8000
            domain = "donkey.helm"
        "#;

    let config: Config = toml::from_str(toml).expect("failed to parse");

    assert_eq!(config.service.len(), 2);
    let app = config
        .service
        .iter()
        .find(|svc| svc.kind == Kind::App)
        .expect("app service present");
    assert_eq!(app.name, "web");
    assert_eq!(app.domain.as_deref(), Some("donkey.helm"));
}

#[test]
fn parse_app_only_config() {
    let toml = r#"
            container_prefix = "app"

            [[service]]
            name = "web"
            kind = "app"
            driver = "frankenphp"
            image = "dunglas/frankenphp:php8.5"
            host = "127.0.0.1"
            port = 8000
            domain = "donkey.helm"
        "#;

    let config: Config = toml::from_str(toml).expect("failed to parse");
    assert_eq!(config.service.len(), 1);
    assert_eq!(config.service[0].kind, Kind::App);
}

#[test]
fn parse_app_domains_list() {
    let toml = r#"
            container_prefix = "app"

            [[service]]
            name = "web"
            kind = "app"
            driver = "frankenphp"
            image = "dunglas/frankenphp:php8.5"
            host = "127.0.0.1"
            port = 8000
            domains = ["primary.helm", "alt.test", "alt.org"]
        "#;

    let config: Config = toml::from_str(toml).expect("failed to parse");
    let app = &config.service[0];

    assert_eq!(app.domain, None);
    assert_eq!(
        app.domains.as_ref().expect("domains configured"),
        &vec![
            "primary.helm".to_owned(),
            "alt.test".to_owned(),
            "alt.org".to_owned()
        ]
    );
    assert_eq!(app.primary_domain(), Some("primary.helm"));
}
