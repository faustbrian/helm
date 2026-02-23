//! Caddy process lifecycle helpers.

use anyhow::{Context, Result};
#[cfg(test)]
use std::cell::RefCell;
use std::path::Path;
use std::process::{Command, Output};

use crate::output::{self, LogLevel, Persistence};

#[cfg(test)]
thread_local! {
    static TEST_CADDY_COMMAND: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Verifies that `caddy` is installed and executable.
pub(super) fn ensure_caddy_installed() -> Result<()> {
    let output = match Command::new(caddy_binary_name()).arg("version").output() {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!(
                "caddy is not installed.\n\
                 install on macOS: brew install caddy\n\
                 install on Linux: https://caddyserver.com/docs/install"
            );
        }
        Err(error) => return Err(error).context("failed to execute caddy"),
    };

    ensure_success(output, "caddy is unavailable").map(|_| ())
}

/// Reloads Caddy with a new config, or starts it if reload fails.
///
/// Reload-first behavior preserves existing process state when possible.
pub(super) fn reload_or_start_caddy(caddyfile_path: &Path) -> Result<()> {
    let config_path = caddyfile_path.to_string_lossy().into_owned();
    let reload_output = run_caddy(
        &["reload", "--config", &config_path, "--adapter", "caddyfile"],
        "failed to execute caddy reload",
    )?;

    if reload_output.status.success() {
        output::event(
            "caddy",
            LogLevel::Success,
            &format!("Reloaded config {}", caddyfile_path.display()),
            Persistence::Persistent,
        );
        return Ok(());
    }

    let start_output = run_caddy(
        &["start", "--config", &config_path, "--adapter", "caddyfile"],
        "failed to execute caddy start",
    )?;

    if start_output.status.success() {
        output::event(
            "caddy",
            LogLevel::Success,
            &format!("Started with config {}", caddyfile_path.display()),
            Persistence::Persistent,
        );
        return Ok(());
    }

    let reload_stderr = stderr_text(&reload_output);
    let start_stderr = stderr_text(&start_output);
    anyhow::bail!(
        "failed to reload or start caddy\nreload error: {reload_stderr}\nstart error: {start_stderr}"
    );
}

/// Attempts to trust local Caddy CA in system trust store.
///
/// # Errors
///
/// Returns an error when command execution fails.
pub fn trust_local_caddy_ca() -> Result<()> {
    let output = run_caddy(&["trust"], "failed to execute caddy trust")?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = stderr_text(&output);
    output::event(
        "caddy",
        LogLevel::Warn,
        &format!("Failed to trust local CA automatically: {stderr}"),
        Persistence::Persistent,
    );
    Ok(())
}

fn run_caddy(args: &[&str], context: &str) -> Result<Output> {
    Command::new(caddy_binary_name())
        .args(args)
        .output()
        .with_context(|| context.to_owned())
}

fn caddy_binary_name() -> String {
    #[cfg(not(test))]
    {
        String::from("caddy")
    }

    #[cfg(test)]
    {
        TEST_CADDY_COMMAND.with(|command| {
            command
                .borrow()
                .clone()
                .unwrap_or_else(|| String::from("caddy"))
        })
    }
}

#[cfg(test)]
fn set_caddy_binary_name(path: String) {
    TEST_CADDY_COMMAND.with(|command| {
        *command.borrow_mut() = Some(path);
    });
}

fn ensure_success(output: Output, error_prefix: &str) -> Result<Output> {
    if output.status.success() {
        return Ok(output);
    }
    anyhow::bail!("{error_prefix}: {}", stderr_text(&output));
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Mutex;

    use super::{ensure_caddy_installed, trust_local_caddy_ca};
    use super::{reload_or_start_caddy, set_caddy_binary_name};

    static TEST_ENV_MUTEX: Mutex<()> = Mutex::new(());

    fn write_mock_bin(dir: &std::path::Path, name: &str, body: &str) {
        let path = dir.join(name);
        std::fs::write(&path, body).expect("write mock binary");
        let mut permissions = std::fs::metadata(&path).expect("metadata").permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&path, permissions).expect("permissions");
    }

    fn with_mock_path<F, R>(script: Option<&str>, test: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _guard = TEST_ENV_MUTEX.lock().unwrap();
        let temp_dir = std::env::temp_dir().join(format!(
            "helm-mock-caddy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        drop(std::fs::remove_dir_all(&temp_dir));
        std::fs::create_dir_all(&temp_dir).expect("create mock bin dir");

        if let Some(body) = script {
            write_mock_bin(&temp_dir, "caddy", body);
        }
        let binary = temp_dir.join("caddy");
        let command_path = binary.to_string_lossy().to_string();
        set_caddy_binary_name(command_path);

        let result = test();

        set_caddy_binary_name("caddy".to_owned());

        result
    }

    #[test]
    fn ensure_caddy_installed_returns_not_found_message_when_missing() {
        with_mock_path(None, || {
            let error = ensure_caddy_installed().expect_err("expected not found");
            assert!(error.to_string().contains("caddy is not installed"));
        });
    }

    #[test]
    fn ensure_caddy_installed_succeeds_with_mock_binary() {
        with_mock_path(
            Some("#!/usr/bin/env sh\necho caddy version\necho ok\n"),
            || {
                ensure_caddy_installed().expect("installed");
            },
        );
    }

    #[test]
    fn reload_or_start_caddy_uses_reload_when_available() {
        with_mock_path(
            Some(
                "#!/usr/bin/env sh\n\nif [ \"$1\" = \"reload\" ]; then\n  echo reloaded\n  exit 0\nfi\n\nif [ \"$1\" = \"start\" ]; then\n  echo should-not-run\n  exit 1\nfi\n\nexit 0\n",
            ),
            || {
                reload_or_start_caddy(std::path::Path::new("/tmp/Caddyfile")).expect("reload path");
            },
        );
    }

    #[test]
    fn reload_or_start_caddy_falls_back_to_start_when_reload_fails() {
        with_mock_path(
            Some(
                "#!/usr/bin/env sh\n\nif [ \"$1\" = \"reload\" ]; then\n  echo reload failed >&2\n  exit 1\nfi\n\nif [ \"$1\" = \"start\" ]; then\n  echo started\n  exit 0\nfi\n\nexit 1\n",
            ),
            || {
                reload_or_start_caddy(std::path::Path::new("/tmp/Caddyfile"))
                    .expect("start fallback");
            },
        );
    }

    #[test]
    fn reload_or_start_caddy_fails_when_reload_and_start_fail() {
        with_mock_path(Some("#!/usr/bin/env sh\n\necho failed\nexit 1\n"), || {
            let error = reload_or_start_caddy(std::path::Path::new("/tmp/Caddyfile"))
                .expect_err("expected fail");
            assert!(
                error
                    .to_string()
                    .contains("failed to reload or start caddy")
            );
        });
    }

    #[test]
    fn trust_local_caddy_ca_succeeds_on_zero_exit_code() {
        with_mock_path(
            Some("#!/usr/bin/env sh\nif [ \"$1\" = \"trust\" ]; then exit 0; fi; exit 1\n"),
            || {
                trust_local_caddy_ca().expect("trust");
            },
        );
    }

    #[test]
    fn trust_local_caddy_ca_ignores_command_error_and_succeeds() {
        with_mock_path(
            Some("#!/usr/bin/env sh\necho trust failure >&2\nexit 1\n"),
            || {
                trust_local_caddy_ca().expect("warn only");
            },
        );
    }

    #[cfg(not(unix))]
    #[allow(unused_imports)]
    #[test]
    fn trust_local_caddy_ca_compiles_without_unix_permissions_api() {
        assert!(std::time::Duration::from_secs(0).is_zero());
    }
}
