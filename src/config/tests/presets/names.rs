use super::*;
use std::collections::HashSet;

#[test]
fn preset_names_are_unique_and_include_primary_aliases() {
    let names = preset_names();
    let unique: HashSet<_> = names.iter().copied().collect();

    assert_eq!(names.len(), unique.len());

    for expected in [
        "mongodb",
        "postgres",
        "pg",
        "pgsql",
        "sqlserver",
        "mssql",
        "mysql",
        "mariadb",
        "redis",
        "valkey",
        "dragonfly",
        "memcached",
        "minio",
        "localstack",
        "rustfs",
        "meilisearch",
        "typesense",
        "frankenphp",
        "laravel",
        "reverb",
        "horizon",
        "queue-worker",
        "queue",
        "scheduler",
        "dusk",
        "selenium",
        "gotenberg",
        "mailhog",
        "mailpit",
        "rabbitmq",
        "soketi",
    ] {
        assert!(names.contains(&expected));
    }
}
