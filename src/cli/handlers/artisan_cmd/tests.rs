//! cli handlers artisan cmd tests module.
//!
//! Contains cli handlers artisan cmd tests logic used by Helm command workflows.

use super::{
    build_artisan_command, build_artisan_test_command, ensure_artisan_ansi_flag,
    is_artisan_test_command, remove_artisan_env_overrides, resolve_artisan_tty,
};
use std::collections::HashMap;

#[test]
fn detects_artisan_test_subcommand() {
    assert!(is_artisan_test_command(&["test".to_owned()]));
    assert!(is_artisan_test_command(&[
        "TeSt".to_owned(),
        "--filter=Foo".to_owned()
    ]));
    assert!(!is_artisan_test_command(&["migrate".to_owned()]));
    assert!(!is_artisan_test_command(&[]));
}

/// Removes artisan env overrides strips only env options as part of the cli handlers artisan cmd tests workflow.
#[test]
fn remove_artisan_env_overrides_strips_only_env_options() {
    let command = vec![
        "test".to_owned(),
        "--env=local".to_owned(),
        "--filter=Invoice".to_owned(),
        "--env".to_owned(),
        "staging".to_owned(),
        "--verbose".to_owned(),
    ];

    assert_eq!(
        remove_artisan_env_overrides(&command),
        vec![
            "test".to_owned(),
            "--filter=Invoice".to_owned(),
            "--verbose".to_owned(),
        ]
    );
}

#[test]
fn artisan_test_defaults_to_non_tty_unless_explicit_tty() {
    assert!(!resolve_artisan_tty(false, false, true));
    assert!(!resolve_artisan_tty(false, true, true));
    assert!(resolve_artisan_tty(true, false, true));
}

#[test]
fn artisan_test_command_sets_memory_limit_to_2048m() {
    let mut inferred_env = HashMap::new();
    inferred_env.insert("DB_HOST".to_owned(), "host.docker.internal".to_owned());

    let command = build_artisan_test_command(vec!["test".to_owned()], &inferred_env);
    assert_eq!(command[0], "sh");
    assert_eq!(command[1], "-lc");
    assert!(command[2].contains("memory_limit=2048M"));
    assert!(command[2].contains("export PHP_INI_SCAN_DIR="));
    assert!(command[2].contains("export DB_HOST='host.docker.internal'"));
    assert!(command[2].contains("export APP_ENV='testing'"));
    assert!(command[2].contains("'php' 'artisan' 'test'"));
}

/// Builds non test artisan command uses plain php invocation for command execution.
#[test]
fn build_non_test_artisan_command_uses_plain_php_invocation() {
    assert_eq!(
        build_artisan_command(vec!["about".to_owned()]),
        vec!["php".to_owned(), "artisan".to_owned(), "about".to_owned(),]
    );
}

#[test]
fn ansi_is_added_by_default() {
    assert_eq!(
        ensure_artisan_ansi_flag(vec!["test".to_owned()]),
        vec!["test".to_owned(), "--ansi".to_owned()]
    );
}

#[test]
fn ansi_flag_is_not_duplicated() {
    assert_eq!(
        ensure_artisan_ansi_flag(vec!["test".to_owned(), "--ansi".to_owned()]),
        vec!["test".to_owned(), "--ansi".to_owned()]
    );
}

#[test]
fn no_ansi_flag_is_removed_and_ansi_is_enforced() {
    assert_eq!(
        ensure_artisan_ansi_flag(vec!["test".to_owned(), "--no-ansi".to_owned()]),
        vec!["test".to_owned(), "--ansi".to_owned()]
    );
}
