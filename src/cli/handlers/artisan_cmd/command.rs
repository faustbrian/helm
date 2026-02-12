use crate::cli;

pub(super) fn is_artisan_test_command(command: &[String]) -> bool {
    command
        .first()
        .is_some_and(|subcommand| subcommand.eq_ignore_ascii_case("test"))
}

pub(super) fn remove_artisan_env_overrides(command: &[String]) -> Vec<String> {
    let mut cleaned = Vec::with_capacity(command.len());
    let mut skip_next = false;
    for arg in command {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--env" {
            skip_next = true;
            continue;
        }
        if arg.starts_with("--env=") {
            continue;
        }
        cleaned.push(arg.clone());
    }
    cleaned
}

pub(super) fn build_artisan_command(user_command: Vec<String>) -> Vec<String> {
    let mut full_command = vec!["php".to_owned()];
    full_command.push("artisan".to_owned());
    full_command.extend(user_command);
    full_command
}

pub(super) fn ensure_artisan_ansi_flag(command: Vec<String>) -> Vec<String> {
    let mut normalized: Vec<String> = command
        .into_iter()
        .filter(|arg| arg != "--no-ansi")
        .collect();

    if !normalized.iter().any(|arg| arg == "--ansi") {
        normalized.push("--ansi".to_owned());
    }

    normalized
}

pub(super) fn resolve_artisan_tty(tty: bool, no_tty: bool, is_test_command: bool) -> bool {
    if is_test_command && !tty {
        return false;
    }
    cli::support::resolve_tty(tty, no_tty)
}
