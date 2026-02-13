//! cli handlers artisan cmd command module.
//!
//! Contains cli handlers artisan cmd command logic used by Helm command workflows.

use crate::cli;

/// Returns whether the artisan command is a `test` invocation.
pub(super) fn is_artisan_test_command(command: &[String]) -> bool {
    command
        .first()
        .is_some_and(|subcommand| subcommand.eq_ignore_ascii_case("test"))
}

/// Removes artisan env overrides as part of the cli handlers artisan cmd command workflow.
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

/// Builds artisan command for command execution.
pub(super) fn build_artisan_command(user_command: Vec<String>) -> Vec<String> {
    let mut full_command = vec!["php".to_owned()];
    full_command.push("artisan".to_owned());
    full_command.extend(user_command);
    full_command
}

/// Ensures artisan ansi flag exists and is in the required state.
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

/// Resolves artisan tty using configured inputs and runtime state.
pub(super) fn resolve_artisan_tty(tty: bool, no_tty: bool, is_test_command: bool) -> bool {
    if is_test_command && !tty {
        return false;
    }
    cli::support::resolve_tty(tty, no_tty)
}
