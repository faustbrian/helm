use super::*;

#[test]
fn preset_allows_overrides() {
    let toml = r#"
            container_prefix = "acme-api"

            [[service]]
            preset = "redis"
            name = "cache"
            port = 41000
            image = "redis:7"
        "#;

    let raw: RawConfig = toml::from_str(toml).expect("raw config parse");
    let config = expansion::expand_raw_config(raw).expect("expand preset config");
    let cache = config.service.first().expect("cache service");
    assert_eq!(cache.name, "cache");
    assert_eq!(cache.port, 41000);
    assert_eq!(cache.image, "redis:7");
    assert_eq!(cache.driver, Driver::Redis);
}
