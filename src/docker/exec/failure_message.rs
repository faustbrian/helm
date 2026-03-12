/// Formats command execution failures with container context and exit code.
pub(super) fn command_failed_in_container(container_name: &str, exit_code: Option<i32>) -> String {
    match exit_code {
        Some(code) => format!("Command failed in container '{container_name}' (exit code: {code})"),
        None => format!("Command failed in container '{container_name}' (terminated by signal)"),
    }
}

#[cfg(test)]
mod tests {
    use super::command_failed_in_container;

    #[test]
    fn formats_failure_with_numeric_exit_code() {
        assert_eq!(
            command_failed_in_container("acme-app", Some(12)),
            "Command failed in container 'acme-app' (exit code: 12)"
        );
    }

    #[test]
    fn formats_failure_without_exit_code_as_signal_termination() {
        assert_eq!(
            command_failed_in_container("acme-app", None),
            "Command failed in container 'acme-app' (terminated by signal)"
        );
    }
}
