use super::dispatch_daemon_request::{prepare_database_dump_restore, prepare_project_command};
use super::{IpcEventKind, IpcProjectCommand, UnixDaemonRuntime};
use crate::control_plane::configuration::{RawWorkflowConfig, RawWorkflowMode, RawWorkflowStep};
use crate::control_plane::state::{
    DaemonOperationRecord, DaemonOperationRecordOptions, DaemonOperationStatus,
};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;

const AUTOMATIC_COMMAND_TIMEOUT_SECONDS: u64 = 30 * 60;
const HASH_BUFFER_BYTES: usize = 64 * 1024;

impl UnixDaemonRuntime {
    /// Queues the next due step for each automatic workflow after convergence.
    pub(super) fn schedule_automatic_workflows(&mut self, now_unix_seconds: i64) {
        if !self.engine_reconciliation.is_converged() {
            return;
        }
        let Some(registry) = self.engine_reconciliation.desired_registry().cloned() else {
            return;
        };

        for project in registry.projects() {
            for (workflow_name, workflow) in project.workflows() {
                if workflow.mode() != RawWorkflowMode::Automatic {
                    continue;
                }
                let revision_key = (
                    project.project_directory().to_path_buf(),
                    workflow_name.to_owned(),
                );
                let revision = self
                    .automatic_workflow_revisions
                    .entry(revision_key)
                    .or_insert_with(|| {
                        automatic_workflow_revision(
                            project.project_directory(),
                            workflow_name,
                            workflow,
                        )
                    })
                    .clone();
                let revision = match revision {
                    Ok(revision) => revision,
                    Err(error) => {
                        tracing::error!(
                            project = project.project_name(),
                            workflow = workflow_name,
                            error,
                            "automatic workflow input validation failed"
                        );
                        continue;
                    }
                };

                for (step_index, step) in workflow.steps().iter().enumerate() {
                    let Some(file) = step.file() else {
                        continue;
                    };
                    let restore_id = automatic_operation_id(&revision, step_index, "restore");
                    if !self.ensure_automatic_restore(
                        &restore_id,
                        project.project_directory(),
                        step,
                        file,
                        now_unix_seconds,
                    ) {
                        break;
                    }
                    let (Some(service), Some(connection)) =
                        (step.migration_service(), step.migration_connection())
                    else {
                        continue;
                    };
                    let migrate_id = automatic_operation_id(&revision, step_index, "migrate");
                    if !self.ensure_automatic_migration(
                        &migrate_id,
                        project.project_directory(),
                        service,
                        connection,
                        now_unix_seconds,
                    ) {
                        break;
                    }
                }
            }
        }
    }

    fn ensure_automatic_restore(
        &mut self,
        operation_id: &str,
        project_directory: &Path,
        step: &RawWorkflowStep,
        relative_file: &Path,
        now_unix_seconds: i64,
    ) -> bool {
        match self.automatic_operation_completed(operation_id) {
            Ok(Some(completed)) => return completed,
            Ok(None) => {}
            Err(error) => {
                tracing::error!(operation_id, error, "automatic restore state lookup failed");
                return false;
            }
        }
        let file = match project_directory.join(relative_file).canonicalize() {
            Ok(file) => file,
            Err(error) => {
                tracing::error!(
                    operation_id,
                    file = %relative_file.display(),
                    error = %error,
                    "automatic restore input is unavailable"
                );
                return false;
            }
        };
        let queued = match prepare_database_dump_restore(
            &self.control_plane,
            operation_id,
            project_directory,
            step.service(),
            &file,
            step.archive_entry(),
            step.reset(),
        ) {
            Ok(queued) => queued,
            Err(error) => {
                tracing::error!(operation_id, error, "automatic restore planning failed");
                return false;
            }
        };
        let payload = match queued.payload_json() {
            Ok(payload) => payload,
            Err(error) => {
                tracing::error!(operation_id, error, "automatic restore encoding failed");
                return false;
            }
        };
        if let Err(error) = self.project_restores.enqueue(queued) {
            tracing::error!(operation_id, error = %error, "automatic restore queue is unavailable");
            return false;
        }
        if let Err(error) = self.persist_automatic_operation(
            operation_id,
            "project_restore",
            payload,
            now_unix_seconds,
        ) {
            drop(self.project_restores.remove(operation_id));
            tracing::error!(operation_id, error, "automatic restore persistence failed");
        }

        false
    }

    fn ensure_automatic_migration(
        &mut self,
        operation_id: &str,
        project_directory: &Path,
        service: &str,
        connection: &str,
        now_unix_seconds: i64,
    ) -> bool {
        match self.automatic_operation_completed(operation_id) {
            Ok(Some(completed)) => return completed,
            Ok(None) => {}
            Err(error) => {
                tracing::error!(
                    operation_id,
                    error,
                    "automatic migration state lookup failed"
                );
                return false;
            }
        }
        let command = IpcProjectCommand::Artisan {
            arguments: vec!["migrate".to_owned(), format!("--database={connection}")],
            browser: false,
        };
        let queued = match prepare_project_command(
            &self.control_plane,
            operation_id,
            project_directory,
            service,
            &command,
            AUTOMATIC_COMMAND_TIMEOUT_SECONDS,
        ) {
            Ok(queued) => queued,
            Err(error) => {
                tracing::error!(operation_id, error, "automatic migration planning failed");
                return false;
            }
        };
        let payload = match queued.payload_json() {
            Ok(payload) => payload,
            Err(error) => {
                tracing::error!(operation_id, error, "automatic migration encoding failed");
                return false;
            }
        };
        if let Err(error) = self.project_commands.enqueue(queued) {
            tracing::error!(operation_id, error = %error, "automatic migration queue is unavailable");
            return false;
        }
        if let Err(error) = self.persist_automatic_operation(
            operation_id,
            "project_command",
            payload,
            now_unix_seconds,
        ) {
            drop(self.project_commands.remove(operation_id));
            tracing::error!(
                operation_id,
                error,
                "automatic migration persistence failed"
            );
        }

        false
    }

    fn automatic_operation_completed(&self, operation_id: &str) -> Result<Option<bool>, String> {
        let operation = self
            .control_plane
            .daemon_operation(operation_id)
            .map_err(|error| error.to_string())?;

        Ok(operation.map(|operation| operation.status() == DaemonOperationStatus::Completed))
    }

    fn persist_automatic_operation(
        &mut self,
        operation_id: &str,
        kind: &str,
        payload_json: String,
        now_unix_seconds: i64,
    ) -> Result<(), String> {
        let accepted_json = serde_json::to_string(&IpcEventKind::Accepted)
            .map_err(|error| format!("automatic operation event encoding failed: {error}"))?;
        let operation = DaemonOperationRecord::new(DaemonOperationRecordOptions {
            operation_id: operation_id.to_owned(),
            kind: kind.to_owned(),
            payload_json,
            status: DaemonOperationStatus::Queued,
            created_at_unix_seconds: now_unix_seconds,
            updated_at_unix_seconds: now_unix_seconds,
        });
        let accepted = self
            .control_plane
            .enqueue_daemon_operation(&operation, &accepted_json, self.event_journal.capacity())
            .map_err(|error| error.to_string())?;
        self.event_journal
            .append_record(accepted)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

fn automatic_workflow_revision(
    project_directory: &Path,
    workflow_name: &str,
    workflow: &RawWorkflowConfig,
) -> Result<String, String> {
    let canonical_project = project_directory.canonicalize().map_err(|error| {
        format!(
            "automatic workflow project '{}' is unavailable: {error}",
            project_directory.display()
        )
    })?;
    let mut digest = Sha256::new();
    digest.update(canonical_project.as_os_str().as_encoded_bytes());
    digest.update([0]);
    digest.update(workflow_name.as_bytes());
    digest.update([0]);
    digest.update(
        serde_json::to_vec(workflow)
            .map_err(|error| format!("automatic workflow encoding failed: {error}"))?,
    );
    let mut buffer = vec![0_u8; HASH_BUFFER_BYTES];
    for step in workflow.steps() {
        let Some(relative_file) = step.file() else {
            continue;
        };
        let file_path = project_directory.join(relative_file);
        let canonical = file_path.canonicalize().map_err(|error| {
            format!(
                "automatic workflow input '{}' is unavailable: {error}",
                relative_file.display()
            )
        })?;
        if !canonical.starts_with(&canonical_project) {
            return Err(format!(
                "automatic workflow input '{}' escapes the project directory",
                relative_file.display()
            ));
        }
        let mut file = File::open(&canonical).map_err(|error| {
            format!(
                "automatic workflow input '{}' cannot be opened: {error}",
                relative_file.display()
            )
        })?;
        loop {
            let read = file.read(&mut buffer).map_err(|error| {
                format!(
                    "automatic workflow input '{}' cannot be hashed: {error}",
                    relative_file.display()
                )
            })?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
        }
    }

    Ok(hex::encode(digest.finalize()))
}

fn automatic_operation_id(revision: &str, step_index: usize, action: &str) -> String {
    format!("automatic-workflow:{revision}:{step_index}:{action}")
}

#[cfg(test)]
mod tests {
    use super::{automatic_operation_id, automatic_workflow_revision};
    use crate::control_plane::configuration::parse_project_config;
    use std::path::Path;

    #[test]
    fn workflow_revision_changes_with_exact_dump_contents() {
        let root = std::env::temp_dir().join(format!(
            "stackctl-automatic-workflow-{}",
            std::process::id()
        ));
        drop(std::fs::remove_dir_all(&root));
        std::fs::create_dir_all(root.join("database")).expect("create workflow fixture");
        let config = "schema_version: 8\nproject: api\nservices:\n  db:\n    preset: mysql\nworkflows:\n  sandbox:\n    mode: automatic\n    steps:\n      - type: database_restore\n        service: db\n        file: database/dump.sql\n";
        let parsed = parse_project_config(config, Path::new("/tmp/.stackctl.yaml"))
            .expect("automatic workflow config");
        let workflow = parsed.workflows().get("sandbox").expect("sandbox workflow");
        std::fs::write(root.join("database/dump.sql"), "SELECT 1;\n").expect("write first dump");
        let first = automatic_workflow_revision(&root, "sandbox", workflow)
            .expect("first workflow revision");
        std::fs::write(root.join("database/dump.sql"), "SELECT 2;\n").expect("write second dump");
        let second = automatic_workflow_revision(&root, "sandbox", workflow)
            .expect("second workflow revision");

        assert_ne!(first, second);
        assert_eq!(
            automatic_operation_id(&first, 0, "restore"),
            format!("automatic-workflow:{first}:0:restore")
        );
        std::fs::remove_dir_all(root).expect("remove workflow fixture");
    }
}
