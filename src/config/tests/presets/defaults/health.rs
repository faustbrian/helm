use super::super::*;

#[test]
fn app_presets_apply_default_health_checks() {
    let toml = r#"
            container_prefix = "acme-api"

            [[service]]
            preset = "laravel"
            name = "web"

            [[service]]
            preset = "gotenberg"
            name = "pdf"

            [[service]]
            preset = "mailhog"
            name = "mail"
        "#;

    let raw: RawConfig = toml::from_str(toml).expect("raw config parse");
    let config = expansion::expand_raw_config(raw).expect("expand preset config");

    let web = config
        .service
        .iter()
        .find(|svc| svc.name == "web")
        .expect("web service");
    assert_eq!(web.health_path.as_deref(), Some("/up"));
    assert_eq!(web.health_statuses.as_deref(), Some(&[200][..]));

    let pdf = config
        .service
        .iter()
        .find(|svc| svc.name == "pdf")
        .expect("pdf service");
    assert_eq!(pdf.health_path.as_deref(), Some("/health"));
    assert_eq!(pdf.health_statuses.as_deref(), Some(&[200][..]));

    let mail = config
        .service
        .iter()
        .find(|svc| svc.name == "mail")
        .expect("mail service");
    assert_eq!(mail.health_path.as_deref(), Some("/"));
    assert_eq!(mail.health_statuses.as_deref(), Some(&[200][..]));
}
