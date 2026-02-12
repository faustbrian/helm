use super::*;

#[test]
fn preset_aliases_cover_all_primary_backends() {
    let toml = r#"
            container_prefix = "acme-api"

            [[service]]
            preset = "postgres"
            name = "pg"

            [[service]]
            preset = "valkey"
            name = "cache"

            [[service]]
            preset = "minio"
            name = "storage"

            [[service]]
            preset = "meilisearch"
            name = "search-meili"

            [[service]]
            preset = "typesense"
            name = "search-typesense"

            [[service]]
            preset = "frankenphp"
            name = "web"

            [[service]]
            preset = "mariadb"
            name = "maria"
        "#;

    let raw: RawConfig = toml::from_str(toml).expect("raw config parse");
    let config = expansion::expand_raw_config(raw).expect("expand preset config");

    let pg = config
        .service
        .iter()
        .find(|svc| svc.name == "pg")
        .expect("pg service");
    assert_eq!(pg.driver, Driver::Postgres);

    let cache = config
        .service
        .iter()
        .find(|svc| svc.name == "cache")
        .expect("cache service");
    assert_eq!(cache.driver, Driver::Valkey);

    let storage = config
        .service
        .iter()
        .find(|svc| svc.name == "storage")
        .expect("storage service");
    assert_eq!(storage.driver, Driver::Minio);

    let meili = config
        .service
        .iter()
        .find(|svc| svc.name == "search-meili")
        .expect("meili service");
    assert_eq!(meili.driver, Driver::Meilisearch);

    let typesense = config
        .service
        .iter()
        .find(|svc| svc.name == "search-typesense")
        .expect("typesense service");
    assert_eq!(typesense.driver, Driver::Typesense);

    let web = config
        .service
        .iter()
        .find(|svc| svc.name == "web")
        .expect("web service");
    assert_eq!(web.driver, Driver::Frankenphp);

    let maria = config
        .service
        .iter()
        .find(|svc| svc.name == "maria")
        .expect("maria service");
    assert_eq!(maria.driver, Driver::Mysql);
    assert_eq!(maria.image, "mariadb:11");
}
