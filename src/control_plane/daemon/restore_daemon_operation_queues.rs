use super::ipc::{IpcEventJournal, IpcEventKind};
use super::{
    MigrationDecisionQueue, PostgresPruneQueue, ProjectBackupQueue, ProjectCommandQueue,
    ProjectRestoreQueue, QueuedMigrationDecision, QueuedPostgresPrune, QueuedProjectBackup,
    QueuedProjectCommand, QueuedProjectRestore,
};
use crate::control_plane::state::{
    DaemonOperationStatus, DaemonOperationTransitionOptions, StateStore, StateStoreError,
};

/// Restores replay-safe queued work and terminalizes ambiguous in-flight work.
pub(crate) fn restore_daemon_operation_queues<Store>(
    store: &mut Store,
    now_unix_seconds: i64,
) -> Result<
    (
        ProjectCommandQueue,
        ProjectBackupQueue,
        PostgresPruneQueue,
        ProjectRestoreQueue,
        MigrationDecisionQueue,
    ),
    StateStoreError,
>
where
    Store: StateStore,
{
    let operations = store.active_daemon_operations()?;
    let mut commands = ProjectCommandQueue::default();
    let mut backups = ProjectBackupQueue::default();
    let mut prunes = PostgresPruneQueue::default();
    let mut restores = ProjectRestoreQueue::default();
    let mut decisions = MigrationDecisionQueue::default();
    let event_capacity = IpcEventJournal::default().capacity();
    for operation in operations {
        if operation.status() == DaemonOperationStatus::Running {
            if operation.kind() == "postgres_prune" {
                restore_running_postgres_prune(
                    store,
                    &mut prunes,
                    operation.operation_id(),
                    operation.payload_json(),
                    now_unix_seconds,
                    event_capacity,
                )?;

                continue;
            }
            let (code, message) = interrupted_diagnostic(operation.kind());
            fail_operation(
                store,
                operation.operation_id(),
                DaemonOperationStatus::Running,
                code,
                message,
                now_unix_seconds,
                event_capacity,
            )?;

            continue;
        }
        let result = match operation.kind() {
            "project_command" => QueuedProjectCommand::from_payload_json(
                operation.operation_id().to_owned(),
                operation.payload_json(),
            )
            .and_then(|queued| commands.enqueue(queued).map_err(|error| error.to_string())),
            "project_backup" => QueuedProjectBackup::from_payload_json(
                operation.operation_id().to_owned(),
                operation.payload_json(),
            )
            .and_then(|queued| backups.enqueue(queued).map_err(|error| error.to_string())),
            "postgres_prune" => QueuedPostgresPrune::from_payload_json(
                operation.operation_id().to_owned(),
                operation.payload_json(),
            )
            .and_then(|queued| prunes.enqueue(queued).map_err(|error| error.to_string())),
            "project_restore" => QueuedProjectRestore::from_payload_json(
                operation.operation_id().to_owned(),
                operation.payload_json(),
            )
            .and_then(|queued| restores.enqueue(queued).map_err(|error| error.to_string())),
            "migration_decision" => QueuedMigrationDecision::from_payload_json(
                operation.operation_id().to_owned(),
                operation.payload_json(),
            )
            .and_then(|queued| decisions.enqueue(queued).map_err(|error| error.to_string())),
            _ => Err("the queued daemon operation kind is unsupported by this build".to_owned()),
        };
        if let Err(error) = result {
            fail_operation(
                store,
                operation.operation_id(),
                DaemonOperationStatus::Queued,
                "operation_payload_invalid",
                &error,
                now_unix_seconds,
                event_capacity,
            )?;
        }
    }

    Ok((commands, backups, prunes, restores, decisions))
}

fn restore_running_postgres_prune<Store>(
    store: &mut Store,
    prunes: &mut PostgresPruneQueue,
    operation_id: &str,
    payload_json: &str,
    now_unix_seconds: i64,
    event_capacity: usize,
) -> Result<(), StateStoreError>
where
    Store: StateStore,
{
    let queued = match QueuedPostgresPrune::from_payload_json(operation_id.to_owned(), payload_json)
    {
        Ok(queued) => queued,
        Err(error) => {
            return fail_operation(
                store,
                operation_id,
                DaemonOperationStatus::Running,
                "operation_payload_invalid",
                &error,
                now_unix_seconds,
                event_capacity,
            );
        }
    };
    let resource_present = store
        .logical_resources()?
        .iter()
        .any(|resource| resource.logical_resource_id() == queued.logical_resource_id());
    let credential_present = store
        .credentials()?
        .iter()
        .any(|credential| credential.credential_id() == queued.credential_id());
    match (resource_present, credential_present) {
        (false, false) => {
            complete_recovered_postgres_prune(store, operation_id, now_unix_seconds, event_capacity)
        }
        (true, true) => {
            drop(
                store.transition_daemon_operation(DaemonOperationTransitionOptions {
                    operation_id,
                    expected: DaemonOperationStatus::Running,
                    next: DaemonOperationStatus::Queued,
                    updated_at_unix_seconds: now_unix_seconds,
                    event_kind_json: None,
                    event_retention_limit: event_capacity,
                })?,
            );
            prunes
                .enqueue(queued)
                .map_err(|error| StateStoreError::InvalidDaemonOperation {
                    detail: error.to_string(),
                })
        }
        _ => fail_operation(
            store,
            operation_id,
            DaemonOperationStatus::Running,
            "postgres_prune_state_incomplete",
            "logical prune found only part of its exact durable state; manual inspection is required",
            now_unix_seconds,
            event_capacity,
        ),
    }
}

fn complete_recovered_postgres_prune<Store>(
    store: &mut Store,
    operation_id: &str,
    now_unix_seconds: i64,
    event_capacity: usize,
) -> Result<(), StateStoreError>
where
    Store: StateStore,
{
    let kind_json = serde_json::to_string(&IpcEventKind::Completed).map_err(|error| {
        StateStoreError::InvalidDaemonOperation {
            detail: format!("failed to encode recovered prune completion: {error}"),
        }
    })?;
    drop(
        store.transition_daemon_operation(DaemonOperationTransitionOptions {
            operation_id,
            expected: DaemonOperationStatus::Running,
            next: DaemonOperationStatus::Completed,
            updated_at_unix_seconds: now_unix_seconds,
            event_kind_json: Some(&kind_json),
            event_retention_limit: event_capacity,
        })?,
    );

    Ok(())
}

fn interrupted_diagnostic(kind: &str) -> (&'static str, &'static str) {
    match kind {
        "project_command" => (
            "project_command_interrupted",
            "the daemon restarted while the project command was running; the command was not replayed",
        ),
        "project_backup" => (
            "project_backup_interrupted",
            "the daemon restarted while the project backup was running; incomplete output was discarded and the backup was not replayed",
        ),
        "postgres_prune" => (
            "postgres_prune_interrupted",
            "the daemon restarted during logical prune; exact retained state must be inspected before retry",
        ),
        "project_restore" => (
            "project_restore_interrupted",
            "the daemon restarted while the project restore was running; retained target state requires explicit recovery",
        ),
        "migration_decision" => (
            "migration_decision_interrupted",
            "the daemon restarted while a migration decision was running; inspect the durable migration phase before retrying",
        ),
        _ => (
            "operation_interrupted",
            "the daemon restarted while an unsupported operation was running; the operation was not replayed",
        ),
    }
}

fn fail_operation<Store>(
    store: &mut Store,
    operation_id: &str,
    expected: DaemonOperationStatus,
    code: &str,
    message: &str,
    now_unix_seconds: i64,
    event_capacity: usize,
) -> Result<(), StateStoreError>
where
    Store: StateStore,
{
    let kind_json = serde_json::to_string(&IpcEventKind::Failed {
        code: code.to_owned(),
        message: message.to_owned(),
    })
    .map_err(|error| StateStoreError::InvalidDaemonOperation {
        detail: format!("failed to encode restart terminal event: {error}"),
    })?;
    drop(
        store.transition_daemon_operation(DaemonOperationTransitionOptions {
            operation_id,
            expected,
            next: DaemonOperationStatus::Failed,
            updated_at_unix_seconds: now_unix_seconds,
            event_kind_json: Some(&kind_json),
            event_retention_limit: event_capacity,
        })?,
    );

    Ok(())
}
