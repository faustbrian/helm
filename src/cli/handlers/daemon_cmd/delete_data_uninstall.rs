use crate::output::{self, LogLevel, Persistence};
use anyhow::{Context, Result, bail};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const DELETION_TIMEOUT: Duration = Duration::from_secs(6 * 60 * 60);
const DELETION_REQUEST_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const DELETION_POLL_INTERVAL: Duration = Duration::from_millis(100);
const DELETION_COMPLETE_MARKER: &str = ".installation-deleted";
const DELETION_COMPLETE_CONTENTS: &[u8] = b"stackctl-installation-deleted-v1\n";

#[cfg(unix)]
pub(super) fn prepare_delete_data_uninstall(service_installed: bool) -> Result<Option<PathBuf>> {
    use crate::control_plane::default_unix_daemon_runtime_directory;

    let runtime_directory = default_unix_daemon_runtime_directory()?;
    if !runtime_directory.exists() {
        if service_installed {
            bail!(
                "refusing delete-data uninstall because '{}' is missing while the daemon service is installed",
                runtime_directory.display()
            );
        }
        output::event(
            "daemon",
            LogLevel::Info,
            "No installed daemon or runtime state was found; no data was deleted",
            Persistence::Persistent,
        );

        return Ok(None);
    }
    let deletion_complete = deletion_complete_marker_exists(&runtime_directory)?;
    if !service_installed && !deletion_complete {
        bail!(
            "delete-data uninstall requires the installed login service so Stackctl can stop the daemon before removing runtime state"
        );
    }
    if !deletion_complete {
        begin_or_resume_installation_deletion()?;
        super::trust::remove_persisted_daemon_trust()?;
        write_deletion_complete_marker(&runtime_directory)?;
    }

    Ok(Some(runtime_directory))
}

#[cfg(unix)]
fn begin_or_resume_installation_deletion() -> Result<()> {
    use crate::control_plane::IpcInstallationLifecycle;

    let status = deletion_status()?;
    match status.lifecycle() {
        IpcInstallationLifecycle::Active => begin_installation_deletion()?,
        IpcInstallationLifecycle::Deleting => {
            if let Some(error) = status.blocking_error() {
                bail!("installation deletion is blocked: {error}");
            }
            if status.failed_operation_id().is_some() {
                begin_installation_deletion()?;
            }
        }
        IpcInstallationLifecycle::Deleted => return Ok(()),
    }

    wait_for_installation_deletion()
}

#[cfg(unix)]
fn begin_installation_deletion() -> Result<()> {
    use crate::control_plane::{IpcOutcome, IpcPayload, IpcResult};

    let response = super::send_singleton_request_with_timeout(
        IpcPayload::PlanInstallationDeletion,
        DELETION_REQUEST_TIMEOUT,
    )?;
    let (confirmation_token, logical_count, volume_count) = match response.outcome() {
        IpcOutcome::Success {
            result: IpcResult::InstallationDeletionPlan { plan },
        } => (
            plan.confirmation_token().to_owned(),
            plan.logical_prunes().len(),
            plan.volume_deletions().len(),
        ),
        IpcOutcome::Success { .. } => {
            bail!("daemon returned an unexpected installation deletion plan")
        }
        IpcOutcome::Failure { diagnostics } => bail!(
            "installation deletion preflight failed: {}",
            deletion_diagnostics(diagnostics)
        ),
    };
    output::event(
        "daemon",
        LogLevel::Info,
        &format!(
            "Deleting {logical_count} retained logical service(s) and {volume_count} persistent volume(s) through verified recovery adapters"
        ),
        Persistence::Persistent,
    );
    let response = super::send_singleton_request_with_timeout(
        IpcPayload::ExecuteInstallationDeletion { confirmation_token },
        DELETION_REQUEST_TIMEOUT,
    )?;
    match response.outcome() {
        IpcOutcome::Success {
            result: IpcResult::InstallationDeletionStarted,
        } => Ok(()),
        IpcOutcome::Success { .. } => {
            bail!("daemon returned an unexpected installation deletion confirmation")
        }
        IpcOutcome::Failure { diagnostics } => bail!(
            "installation deletion confirmation failed: {}",
            deletion_diagnostics(diagnostics)
        ),
    }
}

#[cfg(unix)]
fn deletion_status() -> Result<crate::control_plane::IpcInstallationDeletionStatus> {
    use crate::control_plane::{IpcOutcome, IpcPayload, IpcResult};

    let response = super::send_singleton_request_with_timeout(
        IpcPayload::InstallationDeletionStatus,
        DELETION_REQUEST_TIMEOUT,
    )?;
    match response.outcome() {
        IpcOutcome::Success {
            result: IpcResult::InstallationDeletionStatus { status },
        } => Ok(status.clone()),
        IpcOutcome::Success { .. } => {
            bail!("daemon returned an unexpected installation deletion status")
        }
        IpcOutcome::Failure { diagnostics } => bail!(
            "installation deletion status failed: {}",
            deletion_diagnostics(diagnostics)
        ),
    }
}

#[cfg(unix)]
fn wait_for_installation_deletion() -> Result<()> {
    use crate::control_plane::IpcInstallationLifecycle;

    let deadline = Instant::now() + DELETION_TIMEOUT;
    loop {
        let status = deletion_status()?;
        if let Some(error) = status.blocking_error() {
            bail!("installation deletion is blocked: {error}");
        }
        if let Some(operation_id) = status.failed_operation_id() {
            bail!(
                "installation deletion operation '{operation_id}' failed; rerun the confirmed delete-data command to revalidate and retry it"
            );
        }
        match status.lifecycle() {
            IpcInstallationLifecycle::Deleted => return Ok(()),
            IpcInstallationLifecycle::Deleting => {}
            IpcInstallationLifecycle::Active => {
                bail!("installation deletion returned to the active lifecycle")
            }
        }
        if Instant::now() >= deadline {
            bail!(
                "timed out waiting for installation deletion with {} logical resource(s) and active operations [{}]",
                status.remaining_logical_resources(),
                status.active_operation_ids().join(", ")
            );
        }
        std::thread::sleep(DELETION_POLL_INTERVAL);
    }
}

#[cfg(unix)]
fn deletion_diagnostics(diagnostics: &[crate::control_plane::IpcDiagnostic]) -> String {
    diagnostics
        .iter()
        .map(|diagnostic| format!("{}: {}", diagnostic.code(), diagnostic.message()))
        .collect::<Vec<_>>()
        .join("; ")
}

fn write_deletion_complete_marker(runtime_directory: &Path) -> Result<()> {
    let _directory_lock =
        crate::control_plane::lock_directory(runtime_directory).with_context(|| {
            format!(
                "failed to lock runtime directory {} for terminal deletion marker publication",
                runtime_directory.display()
            )
        })?;
    let marker = runtime_directory.join(DELETION_COMPLETE_MARKER);
    let temporary = runtime_directory.join(".installation-deleted.tmp");
    let marker_exists = deletion_complete_marker_exists(runtime_directory)?;
    match std::fs::remove_file(&temporary) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(error).with_context(|| {
                format!(
                    "failed to remove interrupted terminal deletion marker {}",
                    temporary.display()
                )
            });
        }
    }
    if marker_exists {
        std::fs::File::open(runtime_directory)?.sync_all()?;
        return Ok(());
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .with_context(|| format!("failed to create {}", temporary.display()))?;
    file.write_all(DELETION_COMPLETE_CONTENTS)?;
    file.sync_all()?;
    std::fs::rename(&temporary, &marker).with_context(|| {
        format!(
            "failed to publish terminal deletion marker {}",
            marker.display()
        )
    })?;
    std::fs::File::open(runtime_directory)?.sync_all()?;

    Ok(())
}

fn deletion_complete_marker_exists(runtime_directory: &Path) -> Result<bool> {
    let runtime_metadata = match std::fs::symlink_metadata(runtime_directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    require_owned_runtime_directory(runtime_directory, &runtime_metadata)?;
    let marker = runtime_directory.join(DELETION_COMPLETE_MARKER);
    let metadata = match std::fs::symlink_metadata(&marker) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        bail!(
            "terminal deletion marker '{}' is not a regular owned file",
            marker.display()
        );
    }
    let mut contents = Vec::new();
    std::fs::File::open(&marker)?.read_to_end(&mut contents)?;
    if contents != DELETION_COMPLETE_CONTENTS {
        bail!(
            "terminal deletion marker '{}' has unexpected contents",
            marker.display()
        );
    }

    Ok(true)
}

pub(super) fn remove_deleted_runtime_directory(runtime_directory: &Path) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(runtime_directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    require_owned_runtime_directory(runtime_directory, &metadata)?;
    if !deletion_complete_marker_exists(runtime_directory)? {
        bail!(
            "refusing to remove '{}' without its terminal deletion marker",
            runtime_directory.display()
        );
    }
    std::fs::remove_dir_all(runtime_directory)
        .with_context(|| format!("failed to remove {}", runtime_directory.display()))
}

fn require_owned_runtime_directory(
    runtime_directory: &Path,
    metadata: &std::fs::Metadata,
) -> Result<()> {
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        bail!(
            "Stackctl runtime path '{}' is not an owned directory",
            runtime_directory.display()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_marker_is_required_before_runtime_data_removal() {
        let root = std::env::temp_dir().join(format!(
            "stackctl-delete-data-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        std::fs::create_dir(&root).expect("runtime fixture");

        let error = remove_deleted_runtime_directory(&root)
            .expect_err("unmarked runtime directory must be preserved");
        assert!(error.to_string().contains("terminal deletion marker"));
        let interrupted = root.join(".installation-deleted.tmp");
        std::fs::write(&interrupted, "partial marker").expect("interrupted marker write");
        write_deletion_complete_marker(&root).expect("write terminal marker");
        assert!(!interrupted.exists());
        std::fs::write(root.join("state.sqlite3"), b"state").expect("state fixture");
        remove_deleted_runtime_directory(&root).expect("remove marked runtime directory");
        assert!(!root.exists());
    }

    #[cfg(unix)]
    #[test]
    fn terminal_marker_waits_for_the_runtime_directory_lock() {
        use std::sync::mpsc;

        let root = std::env::temp_dir().join(format!(
            "stackctl-delete-data-lock-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        std::fs::create_dir(&root).expect("runtime fixture");
        let directory_lock =
            crate::control_plane::lock_directory(&root).expect("lock runtime directory");
        let (sender, receiver) = mpsc::channel();
        let root_for_thread = root.clone();
        let marker_thread = std::thread::spawn(move || {
            sender
                .send(write_deletion_complete_marker(&root_for_thread))
                .expect("report marker result");
        });

        assert!(receiver.recv_timeout(Duration::from_millis(100)).is_err());
        directory_lock.unlock().expect("unlock runtime directory");
        receiver
            .recv_timeout(Duration::from_secs(2))
            .expect("marker resumes after directory unlock")
            .expect("write terminal marker");
        marker_thread.join().expect("join marker thread");

        std::fs::remove_dir_all(root).expect("remove runtime fixture");
    }

    #[cfg(unix)]
    #[test]
    fn runtime_symlinks_are_never_followed_for_delete_data() {
        use std::os::unix::fs::symlink;

        let target = std::env::temp_dir().join(format!(
            "stackctl-delete-data-target-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let link = target.with_extension("link");
        std::fs::create_dir(&target).expect("symlink target");
        std::fs::write(
            target.join(DELETION_COMPLETE_MARKER),
            DELETION_COMPLETE_CONTENTS,
        )
        .expect("target marker");
        symlink(&target, &link).expect("runtime symlink");

        let error =
            remove_deleted_runtime_directory(&link).expect_err("runtime symlink must be refused");
        assert!(error.to_string().contains("not an owned directory"));
        assert!(target.exists());

        std::fs::remove_file(link).expect("remove symlink");
        std::fs::remove_dir_all(target).expect("remove symlink target");
    }
}
