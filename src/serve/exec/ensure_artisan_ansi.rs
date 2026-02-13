//! Normalization helper for `php artisan` ANSI flags.

/// Forces `--ansi` for artisan commands and strips `--no-ansi`.
pub(super) fn ensure_artisan_ansi(command: Vec<String>) -> Vec<String> {
    if command.first().map(String::as_str) != Some("php")
        || command.get(1).map(String::as_str) != Some("artisan")
    {
        return command;
    }

    let mut normalized: Vec<String> = command
        .into_iter()
        .filter(|arg| arg != "--no-ansi")
        .collect();
    if !normalized.iter().any(|arg| arg == "--ansi") {
        normalized.push("--ansi".to_owned());
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::ensure_artisan_ansi;

    #[test]
    fn non_artisan_commands_are_unchanged() {
        let command = vec!["php".to_owned(), "-v".to_owned()];
        assert_eq!(ensure_artisan_ansi(command.clone()), command);
    }

    #[test]
    fn artisan_commands_force_ansi() {
        let command = vec!["php".to_owned(), "artisan".to_owned(), "about".to_owned()];
        assert_eq!(
            ensure_artisan_ansi(command),
            vec![
                "php".to_owned(),
                "artisan".to_owned(),
                "about".to_owned(),
                "--ansi".to_owned(),
            ]
        );
    }

    #[test]
    fn artisan_no_ansi_is_replaced() {
        let command = vec![
            "php".to_owned(),
            "artisan".to_owned(),
            "about".to_owned(),
            "--no-ansi".to_owned(),
        ];
        assert_eq!(
            ensure_artisan_ansi(command),
            vec![
                "php".to_owned(),
                "artisan".to_owned(),
                "about".to_owned(),
                "--ansi".to_owned(),
            ]
        );
    }
}
