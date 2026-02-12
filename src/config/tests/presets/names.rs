use super::*;
use std::collections::HashSet;

#[test]
fn preset_names_are_unique_and_include_primary_aliases() {
    let names = preset_names();
    let unique: HashSet<_> = names.iter().copied().collect();

    assert_eq!(names.len(), unique.len());

    for expected in [
        "postgres",
        "pg",
        "mysql",
        "mariadb",
        "redis",
        "valkey",
        "minio",
        "rustfs",
        "meilisearch",
        "typesense",
        "frankenphp",
        "laravel",
        "laravel-minimal",
        "laravel-full",
        "gotenberg",
        "mailhog",
    ] {
        assert!(names.contains(&expected));
    }
}
