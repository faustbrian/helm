use super::super::*;

#[test]
fn preset_config_expands_defaults_and_assigns_ports() {
    let toml = r#"
            container_prefix = "acme-api"

            [[service]]
            preset = "mysql"
            name = "acme"

            [[service]]
            preset = "mysql"
            name = "billing"

            [[service]]
            preset = "redis"

            [[service]]
            preset = "rustfs"

            [[service]]
            preset = "laravel"
            domain = "acme-api.helm"
        "#;

    let raw: RawConfig = toml::from_str(toml).expect("raw config parse");
    let config = expansion::expand_raw_config(raw).expect("expand preset config");

    assert_eq!(config.service.len(), 5);
    let acme = config
        .service
        .iter()
        .find(|svc| svc.name == "acme")
        .expect("acme service");
    assert_eq!(acme.driver, Driver::Mysql);
    assert_eq!(acme.port, 33060);

    let billing = config
        .service
        .iter()
        .find(|svc| svc.name == "billing")
        .expect("billing service");
    assert_eq!(billing.driver, Driver::Mysql);
    assert_eq!(billing.port, 33061);

    let app = config
        .service
        .iter()
        .find(|svc| svc.kind == Kind::App)
        .expect("app service");
    assert_eq!(app.name, "app");
    assert_eq!(app.port, 33065);
    assert_eq!(app.domain.as_deref(), Some("acme-api.helm"));
    assert_eq!(
        app.env.as_ref().and_then(|env| env.get("APP_ENV")),
        Some(&"local".to_owned())
    );
}

#[test]
fn laravel_preset_forces_local_app_env() {
    let toml = r#"
            container_prefix = "acme-api"

            [[service]]
            preset = "laravel"
            env = { APP_ENV = "production" }
        "#;

    let raw: RawConfig = toml::from_str(toml).expect("raw config parse");
    let config = expansion::expand_raw_config(raw).expect("expand preset config");
    let app = config
        .service
        .iter()
        .find(|svc| svc.kind == Kind::App)
        .expect("app service");

    assert_eq!(
        app.env.as_ref().and_then(|env| env.get("APP_ENV")),
        Some(&"local".to_owned())
    );
}
