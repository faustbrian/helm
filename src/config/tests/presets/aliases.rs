use super::*;

#[test]
fn preset_aliases_cover_all_primary_backends() {
    let toml = r#"
            container_prefix = "acme-api"

            [[service]]
            preset = "mongodb"
            name = "mongo"

            [[service]]
            preset = "postgres"
            name = "pg"

            [[service]]
            preset = "pgsql"
            name = "pgsql"

            [[service]]
            preset = "valkey"
            name = "cache"

            [[service]]
            preset = "memcached"
            name = "mem"

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
            preset = "reverb"
            name = "ws"

            [[service]]
            preset = "mariadb"
            name = "maria"

            [[service]]
            preset = "rabbitmq"
            name = "queue"

            [[service]]
            preset = "soketi"
            name = "socket"

            [[service]]
            preset = "scheduler"
            name = "cron"

            [[service]]
            preset = "queue"
            name = "worker"

            [[service]]
            preset = "selenium"
            name = "browser"

            [[service]]
            preset = "mailpit"
            name = "mailpit"
        "#;

    let raw: RawConfig = toml::from_str(toml).expect("raw config parse");
    let config = expansion::expand_raw_config(raw).expect("expand preset config");

    let mongo = config
        .service
        .iter()
        .find(|svc| svc.name == "mongo")
        .expect("mongo service");
    assert_eq!(mongo.driver, Driver::Mongodb);
    assert_eq!(mongo.image, "mongo:8");

    let pg = config
        .service
        .iter()
        .find(|svc| svc.name == "pg")
        .expect("pg service");
    assert_eq!(pg.driver, Driver::Postgres);

    let pgsql = config
        .service
        .iter()
        .find(|svc| svc.name == "pgsql")
        .expect("pgsql service");
    assert_eq!(pgsql.driver, Driver::Postgres);

    let cache = config
        .service
        .iter()
        .find(|svc| svc.name == "cache")
        .expect("cache service");
    assert_eq!(cache.driver, Driver::Valkey);

    let mem = config
        .service
        .iter()
        .find(|svc| svc.name == "mem")
        .expect("mem service");
    assert_eq!(mem.driver, Driver::Memcached);
    assert_eq!(mem.image, "memcached:1.6-alpine");

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

    let ws = config
        .service
        .iter()
        .find(|svc| svc.name == "ws")
        .expect("ws service");
    assert_eq!(ws.driver, Driver::Reverb);

    let maria = config
        .service
        .iter()
        .find(|svc| svc.name == "maria")
        .expect("maria service");
    assert_eq!(maria.driver, Driver::Mysql);
    assert_eq!(maria.image, "mariadb:11");

    let queue = config
        .service
        .iter()
        .find(|svc| svc.name == "queue")
        .expect("queue service");
    assert_eq!(queue.driver, Driver::Rabbitmq);
    assert_eq!(queue.image, "rabbitmq:3-management");

    let socket = config
        .service
        .iter()
        .find(|svc| svc.name == "socket")
        .expect("socket service");
    assert_eq!(socket.driver, Driver::Soketi);
    assert_eq!(socket.image, "quay.io/soketi/soketi:latest");

    let cron = config
        .service
        .iter()
        .find(|svc| svc.name == "cron")
        .expect("cron service");
    assert_eq!(cron.driver, Driver::Scheduler);
    assert_eq!(cron.image, "dunglas/frankenphp:php8.5");

    let worker = config
        .service
        .iter()
        .find(|svc| svc.name == "worker")
        .expect("worker service");
    assert_eq!(worker.driver, Driver::Frankenphp);
    assert_eq!(worker.image, "dunglas/frankenphp:php8.5");

    let browser = config
        .service
        .iter()
        .find(|svc| svc.name == "browser")
        .expect("browser service");
    assert_eq!(browser.driver, Driver::Dusk);
    assert_eq!(browser.image, "selenium/standalone-chromium:latest");

    let mailpit = config
        .service
        .iter()
        .find(|svc| svc.name == "mailpit")
        .expect("mailpit service");
    assert_eq!(mailpit.driver, Driver::Mailhog);
    assert_eq!(mailpit.image, "axllent/mailpit:latest");
}
