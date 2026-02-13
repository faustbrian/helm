use super::super::inferred_app_env;
use super::helpers::svc;
use crate::config::{Config, Driver, Kind};

#[test]
fn inferred_app_env_includes_core_laravel_service_vars() {
    let mut db = svc("db", Kind::Database, Driver::Mysql, 33060);
    db.database = Some("laravel".to_owned());
    db.username = Some("laravel".to_owned());
    db.password = Some("laravel".to_owned());

    let mut redis = svc("redis", Kind::Cache, Driver::Redis, 6380);
    redis.password = Some("secret".to_owned());

    let mut s3 = svc("s3", Kind::ObjectStore, Driver::Rustfs, 9000);
    s3.bucket = Some("media".to_owned());
    s3.access_key = Some("minio".to_owned());
    s3.secret_key = Some("miniosecret".to_owned());
    s3.region = Some("us-east-1".to_owned());

    let gotenberg = svc("gotenberg", Kind::App, Driver::Gotenberg, 33066);
    let mut mailhog = svc("mailhog", Kind::App, Driver::Mailhog, 33067);
    mailhog.smtp_port = Some(34067);

    let config = Config {
        schema_version: 1,
        container_prefix: Some("app".to_owned()),
        service: vec![db, redis, s3, gotenberg, mailhog],
        swarm: vec![],
    };

    let vars = inferred_app_env(&config);

    assert_eq!(
        vars.get("DB_HOST"),
        Some(&"host.docker.internal".to_owned())
    );
    assert_eq!(vars.get("DB_PORT"), Some(&"33060".to_owned()));
    assert_eq!(
        vars.get("HELM_SQL_CLIENT_FLAVOR"),
        Some(&"mysql".to_owned())
    );
    assert_eq!(vars.get("QUEUE_CONNECTION"), Some(&"redis".to_owned()));
    assert_eq!(
        vars.get("REDIS_HOST"),
        Some(&"host.docker.internal".to_owned())
    );
    assert_eq!(
        vars.get("REDIS_CACHE_HOST"),
        Some(&"host.docker.internal".to_owned())
    );
    assert_eq!(vars.get("REDIS_CACHE_PORT"), Some(&"6380".to_owned()));
    assert_eq!(vars.get("AWS_BUCKET"), Some(&"media".to_owned()));
    assert_eq!(
        vars.get("AWS_URL"),
        Some(&"http://host.docker.internal:9000".to_owned())
    );
    assert_eq!(
        vars.get("GOTENBERG_BASE_URL"),
        Some(&"http://host.docker.internal:33066".to_owned())
    );
    assert_eq!(vars.get("MAIL_MAILER"), Some(&"smtp".to_owned()));
    assert_eq!(
        vars.get("MAIL_HOST"),
        Some(&"host.docker.internal".to_owned())
    );
    assert_eq!(vars.get("MAIL_PORT"), Some(&"34067".to_owned()));
    assert_eq!(vars.get("MAIL_ENCRYPTION"), Some(&"null".to_owned()));
}

#[test]
fn inferred_app_env_uses_mariadb_flavor_for_mariadb_only_database_images() {
    let mut db = svc("db", Kind::Database, Driver::Mysql, 33060);
    db.image = "mariadb:11".to_owned();

    let config = Config {
        schema_version: 1,
        container_prefix: Some("app".to_owned()),
        service: vec![db],
        swarm: vec![],
    };

    let vars = inferred_app_env(&config);
    assert_eq!(
        vars.get("HELM_SQL_CLIENT_FLAVOR"),
        Some(&"mariadb".to_owned())
    );
}
