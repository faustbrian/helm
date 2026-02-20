use super::super::container::resolved_run_command;
use super::super::exec::build_exec_args;
use super::helpers::app_service;

#[test]
fn octane_enabled_uses_default_octane_command() {
    let mut service = app_service();
    service.octane = true;

    let command = resolved_run_command(&service).expect("octane command");
    assert_eq!(
        command,
        vec![
            "php".to_owned(),
            "artisan".to_owned(),
            "octane:frankenphp".to_owned(),
            "--ansi".to_owned(),
            "--watch".to_owned(),
            "--workers=1".to_owned(),
            "--max-requests=1".to_owned(),
            "--host=0.0.0.0".to_owned(),
            "--port=80".to_owned(),
        ]
    );
}

#[test]
fn explicit_command_overrides_octane_default_command() {
    let mut service = app_service();
    service.octane = true;
    service.command = Some(vec![
        "php".to_owned(),
        "artisan".to_owned(),
        "serve".to_owned(),
    ]);

    let command = resolved_run_command(&service).expect("explicit command");
    assert_eq!(
        command,
        vec!["php".to_owned(), "artisan".to_owned(), "serve".to_owned(),]
    );
}

#[test]
fn artisan_exec_args_include_php_artisan_and_flags() {
    let args = build_exec_args(
        "acme-api-serve-app",
        &[
            "php".to_owned(),
            "artisan".to_owned(),
            "migrate".to_owned(),
            "--force".to_owned(),
        ],
        false,
    );

    assert_eq!(
        args,
        vec![
            "exec".to_owned(),
            "-i".to_owned(),
            "acme-api-serve-app".to_owned(),
            "php".to_owned(),
            "artisan".to_owned(),
            "migrate".to_owned(),
            "--force".to_owned(),
        ]
    );
}
