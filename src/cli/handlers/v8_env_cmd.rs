//! Explicit strict v8 managed-environment export through singleton IPC.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};

use crate::cli::args::{Cli, Commands, EnvCommands};
use crate::cli::dispatch::context::CliDispatchContext;
use crate::control_plane::{
    IpcDiagnostic, IpcManagedEnvironment, IpcOutcome, IpcPayload, IpcRequest, IpcResponse,
    IpcResult, default_unix_daemon_runtime_directory, send_unix_request,
};
use crate::output::{self, LogLevel, Persistence};

use super::v8_project::resolve_v8_project;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
static ENVIRONMENT_REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(crate) fn handle_v8_env(cli: &Cli, context: &CliDispatchContext<'_>) -> Result<bool> {
    let Commands::Env(args) = &cli.command else {
        return Ok(false);
    };
    let Some(project) = resolve_v8_project(context)? else {
        return Ok(false);
    };
    let EnvCommands::Generate {
        output: destination,
    } = &args.command;
    if context.dry_run() {
        bail!("--dry-run is not supported for managed secret export");
    }

    let request_id = format!(
        "project-environment-{}-{}",
        std::process::id(),
        ENVIRONMENT_REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let response = send_unix_request(
        &default_unix_daemon_runtime_directory()?.join("daemon.sock"),
        &IpcRequest::new(
            request_id,
            IpcPayload::ProjectEnvironment {
                canonical_path: project.root().to_path_buf(),
            },
        ),
        REQUEST_TIMEOUT,
    )?;
    let environment = managed_environment(&response)?;
    write_managed_environment(destination, environment)?;
    if !context.quiet() {
        output::event(
            "env",
            LogLevel::Success,
            &format!("Exported managed environment to {}", destination.display()),
            Persistence::Persistent,
        );
    }

    Ok(true)
}

fn managed_environment(response: &IpcResponse) -> Result<&IpcManagedEnvironment> {
    match response.outcome() {
        IpcOutcome::Success {
            result: IpcResult::ProjectEnvironment { environment },
        } => Ok(environment),
        IpcOutcome::Success { .. } => bail!("daemon returned an unexpected environment response"),
        IpcOutcome::Failure { diagnostics } => Err(diagnostic_error(diagnostics)),
    }
}

fn diagnostic_error(diagnostics: &[IpcDiagnostic]) -> anyhow::Error {
    let message = diagnostics
        .iter()
        .map(|diagnostic| format!("{}: {}", diagnostic.code(), diagnostic.message()))
        .collect::<Vec<_>>()
        .join("; ");
    anyhow!("daemon request failed: {message}")
}

fn write_managed_environment(path: &Path, environment: &IpcManagedEnvironment) -> Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let metadata = fs::symlink_metadata(parent)
        .with_context(|| format!("failed to inspect output directory {}", parent.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        bail!("managed environment output directory must be a real directory");
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .with_context(|| {
            format!(
                "failed to create new managed environment {}",
                path.display()
            )
        })?;
    let result = (|| -> Result<()> {
        for (key, value) in environment.values() {
            writeln!(file, "{key}={}", quote_env_value(value))?;
        }
        file.sync_all()?;
        Ok(())
    })();
    if let Err(error) = result {
        drop(file);
        drop(fs::remove_file(path));
        return Err(error).context("failed to persist managed environment");
    }

    Ok(())
}

fn quote_env_value(value: &str) -> String {
    let escaped = value.chars().fold(String::new(), |mut output, character| {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            _ => output.push(character),
        }
        output
    });
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use crate::control_plane::IpcManagedEnvironment;

    use super::write_managed_environment;

    #[test]
    fn explicit_export_is_private_sorted_escaped_and_never_overwritten() {
        let root =
            std::env::temp_dir().join(format!("stackctl-v8-env-export-{}", std::process::id()));
        drop(fs::remove_dir_all(&root));
        fs::create_dir(&root).expect("create export root");
        let output = root.join("managed.env");
        let environment = IpcManagedEnvironment::new(
            "bill".to_owned(),
            "sha256:environment".to_owned(),
            BTreeMap::from([
                ("APP_NAME".to_owned(), "Bill \"App\"".to_owned()),
                ("DB_PASSWORD".to_owned(), "line1\nline2".to_owned()),
            ]),
        );

        write_managed_environment(&output, &environment).expect("export environment");

        assert_eq!(
            fs::read_to_string(&output).expect("read environment"),
            "APP_NAME=\"Bill \\\"App\\\"\"\nDB_PASSWORD=\"line1\\nline2\"\n"
        );
        assert_eq!(
            fs::metadata(&output)
                .expect("output metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        let error = write_managed_environment(&output, &environment).expect_err("no overwrite");
        assert!(error.to_string().contains("failed to create new"));

        fs::remove_dir_all(root).expect("remove export fixture");
    }
}
