//! cli handlers app create cmd commands module.
//!
//! Contains cli handlers app create cmd commands logic used by Helm command workflows.

pub(super) fn setup_commands() -> Vec<Vec<String>> {
    vec![
        vec![
            "php".to_owned(),
            "artisan".to_owned(),
            "key:generate".to_owned(),
            "--ansi".to_owned(),
            "--force".to_owned(),
        ],
        vec![
            "php".to_owned(),
            "artisan".to_owned(),
            "optimize:clear".to_owned(),
            "--ansi".to_owned(),
        ],
    ]
}

pub(super) fn storage_link_command() -> Vec<String> {
    vec![
        "php".to_owned(),
        "artisan".to_owned(),
        "storage:link".to_owned(),
        "--ansi".to_owned(),
        "--force".to_owned(),
    ]
}

pub(super) fn migrate_command() -> Vec<String> {
    vec![
        "php".to_owned(),
        "artisan".to_owned(),
        "migrate".to_owned(),
        "--ansi".to_owned(),
        "--force".to_owned(),
    ]
}

pub(super) fn seed_command() -> Vec<String> {
    vec![
        "php".to_owned(),
        "artisan".to_owned(),
        "db:seed".to_owned(),
        "--ansi".to_owned(),
        "--force".to_owned(),
    ]
}
