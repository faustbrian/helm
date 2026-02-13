//! MySQL/MariaDB schema-dump compatibility shims for serve exec flows.

use anyhow::Result;
use std::collections::HashMap;

use crate::config::ServiceConfig;
use crate::serve::sql_client_flavor::sql_client_flavor_from_injected_env;

use super::container::exec_command;

/// Returns shell script that installs a `mysqldump` wrapper in-container.
fn install_wrapper_script(sql_client_flavor: &str) -> String {
    format!(
        r#"cat <<'EOF' >/usr/local/bin/mysqldump
#!/usr/bin/env bash
set -euo pipefail

HELM_SQL_CLIENT_FLAVOR={sql_client_flavor}
real_mysqldump="/usr/bin/mysqldump"
if [[ ! -x "$real_mysqldump" ]]; then
  real_mysqldump="$(command -v mariadb-dump || command -v mysqldump)"
fi

sanitized=()
result_file=""
for arg in "$@"; do
  case "$arg" in
    --column-statistics=0|--set-gtid-purged=OFF)
      continue
      ;;
    --result-file=*)
      result_file="${{arg#--result-file=}}"
      ;;
  esac
  sanitized+=("$arg")
done

stderr_file="$(mktemp)"
if "$real_mysqldump" "${{sanitized[@]}}" 2>"$stderr_file"; then
  if [[ "$HELM_SQL_CLIENT_FLAVOR" == "mysql" && -n "$result_file" && -f "$result_file" ]]; then
    sed -i '/^\/\*M\!.*\*\/;*$/d' "$result_file"
  fi
  rm -f "$stderr_file"
  exit 0
fi
status=$?

if grep -qi "self-signed certificate in certificate chain" "$stderr_file"; then
  if "$real_mysqldump" --help 2>/dev/null | grep -q -- "--ssl-mode"; then
    "$real_mysqldump" --ssl-mode=DISABLED "${{sanitized[@]}}"
  else
    "$real_mysqldump" --skip-ssl "${{sanitized[@]}}"
  fi
  retry_status=$?
  if [[ "$HELM_SQL_CLIENT_FLAVOR" == "mysql" && -n "$result_file" && -f "$result_file" ]]; then
    sed -i '/^\/\*M\!.*\*\/;*$/d' "$result_file"
  fi
  rm -f "$stderr_file"
  exit "$retry_status"
fi

cat "$stderr_file" >&2
rm -f "$stderr_file"
exit "$status"
EOF
chmod +x /usr/local/bin/mysqldump"#
    )
}

/// Returns shell script that installs a `mysql` wrapper in-container.
fn install_mysql_wrapper_script() -> &'static str {
    r#"cat <<'EOF' >/usr/local/bin/mysql
#!/usr/bin/env bash
set -euo pipefail

real_mysql="/usr/bin/mysql"
if [[ ! -x "$real_mysql" ]]; then
  real_mysql="$(command -v mariadb || command -v mysql)"
fi

stderr_file="$(mktemp)"
if "$real_mysql" "$@" 2>"$stderr_file"; then
  rm -f "$stderr_file"
  exit 0
fi
status=$?

if grep -qi "self-signed certificate in certificate chain" "$stderr_file"; then
  if "$real_mysql" --help 2>/dev/null | grep -q -- "--ssl-mode"; then
    "$real_mysql" --ssl-mode=DISABLED "$@"
  else
    "$real_mysql" --skip-ssl "$@"
  fi
  retry_status=$?
  rm -f "$stderr_file"
  exit "$retry_status"
fi

cat "$stderr_file" >&2
rm -f "$stderr_file"
exit "$status"
EOF
chmod +x /usr/local/bin/mysql"#
}

/// Installs the `mysql` wrapper into a running serve container.
pub(super) fn ensure_mysql_cli_compat(target: &ServiceConfig) -> Result<()> {
    let command = vec![
        "sh".to_owned(),
        "-lc".to_owned(),
        install_mysql_wrapper_script().to_owned(),
    ];
    exec_command(target, &command, false)
}

/// Installs `mysqldump` wrapper when about to run `artisan schema:dump`.
pub(super) fn ensure_schema_dump_compat(
    target: &ServiceConfig,
    artisan_args: &[String],
) -> Result<()> {
    if !is_schema_dump_command(artisan_args) {
        return Ok(());
    }

    let command = vec![
        "sh".to_owned(),
        "-lc".to_owned(),
        install_wrapper_script("mysql"),
    ];

    // No TTY required; install script into the running app container
    // before running `php artisan schema:dump`.
    exec_command(target, &command, false)
}

/// Installs `mysqldump` wrapper for direct command execution paths.
pub(super) fn ensure_schema_dump_compat_for_command(
    target: &ServiceConfig,
    command: &[String],
    injected_env: &HashMap<String, String>,
) -> Result<()> {
    if !is_schema_dump_exec_command(command) {
        return Ok(());
    }

    let sql_client_flavor = sql_client_flavor_from_injected_env(injected_env);

    let install = vec![
        "sh".to_owned(),
        "-lc".to_owned(),
        install_wrapper_script(sql_client_flavor.as_str()),
    ];

    exec_command(target, &install, false)
}

/// Returns whether the artisan subcommand is `schema:dump`.
fn is_schema_dump_command(artisan_args: &[String]) -> bool {
    artisan_args
        .first()
        .is_some_and(|arg| arg.eq_ignore_ascii_case("schema:dump"))
}

/// Returns whether a full command vector represents `php artisan schema:dump`.
fn is_schema_dump_exec_command(command: &[String]) -> bool {
    command.first().map(String::as_str) == Some("php")
        && command.get(1).map(String::as_str) == Some("artisan")
        && command
            .get(2)
            .is_some_and(|arg| arg.eq_ignore_ascii_case("schema:dump"))
}

#[cfg(test)]
mod tests {
    use super::{
        install_mysql_wrapper_script, is_schema_dump_command, is_schema_dump_exec_command,
    };

    #[test]
    fn detects_schema_dump_command() {
        assert!(is_schema_dump_command(&["schema:dump".to_owned()]));
    }

    #[test]
    fn ignores_other_artisan_commands() {
        assert!(!is_schema_dump_command(&["migrate".to_owned()]));
    }

    #[test]
    fn ignores_empty_commands() {
        assert!(!is_schema_dump_command(&[]));
    }

    #[test]
    fn detects_schema_dump_exec_command() {
        assert!(is_schema_dump_exec_command(&[
            "php".to_owned(),
            "artisan".to_owned(),
            "schema:dump".to_owned(),
            "--ansi".to_owned(),
        ]));
    }

    #[test]
    fn ignores_non_schema_exec_command() {
        assert!(!is_schema_dump_exec_command(&[
            "php".to_owned(),
            "artisan".to_owned(),
            "migrate".to_owned(),
        ]));
    }

    #[test]
    fn mysql_wrapper_script_disables_ssl_on_self_signed_errors() {
        let script = install_mysql_wrapper_script();
        assert!(script.contains("/usr/local/bin/mysql"));
        assert!(script.contains("--ssl-mode=DISABLED"));
        assert!(script.contains("--skip-ssl"));
    }
}
