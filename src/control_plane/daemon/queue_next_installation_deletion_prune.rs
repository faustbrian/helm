use super::ipc::{IpcEventJournal, IpcEventKind};
use super::{PostgresPruneQueue, QueuedPostgresPrune};
use crate::control_plane::application::ControlPlane;
use crate::control_plane::state::{
    DaemonOperationRecord, DaemonOperationRecordOptions, DaemonOperationStatus,
    InstallationLifecycle, StateStore,
};

/// Durably schedules the next exact tenant deletion, once per deletion plan.
pub(crate) fn queue_next_installation_deletion_prune<Store>(
    control_plane: &mut ControlPlane<Store>,
    queue: &mut PostgresPruneQueue,
    event_journal: &mut IpcEventJournal,
    now_unix_seconds: i64,
) -> Result<Option<String>, String>
where
    Store: StateStore,
{
    if now_unix_seconds < 0 {
        return Err("installation deletion time must not be negative".to_owned());
    }
    if control_plane
        .installation_lifecycle()
        .map_err(|error| error.to_string())?
        != Some(InstallationLifecycle::Deleting)
        || queue.len() > 0
    {
        return Ok(None);
    }
    let deletion = control_plane.plan_installation_deletion()?;
    let Some(plan) = deletion.logical_prunes().first() else {
        return Ok(None);
    };
    let operation_id = format!("installation-delete-{}", deletion.confirmation_token());
    let queued = QueuedPostgresPrune::new(
        operation_id.clone(),
        plan,
        plan.confirmation_token().to_owned(),
    )?;
    let payload_json = queued.payload_json()?;
    if let Some(existing) = control_plane
        .daemon_operation(&operation_id)
        .map_err(|error| error.to_string())?
    {
        if existing.kind() == "postgres_prune" && existing.payload_json() == payload_json {
            return Ok(None);
        }

        return Err(format!(
            "installation deletion operation ID '{operation_id}' is already owned by different intent"
        ));
    }
    queue.enqueue(queued).map_err(|error| error.to_string())?;
    let accepted_kind_json =
        serde_json::to_string(&IpcEventKind::Accepted).map_err(|error| error.to_string())?;
    let operation = DaemonOperationRecord::new(DaemonOperationRecordOptions {
        operation_id: operation_id.clone(),
        kind: "postgres_prune".to_owned(),
        payload_json,
        status: DaemonOperationStatus::Queued,
        created_at_unix_seconds: now_unix_seconds,
        updated_at_unix_seconds: now_unix_seconds,
    });
    let accepted = match control_plane.enqueue_daemon_operation(
        &operation,
        &accepted_kind_json,
        event_journal.capacity(),
    ) {
        Ok(event) => event,
        Err(error) => {
            drop(queue.remove(&operation_id));

            return Err(error.to_string());
        }
    };
    event_journal
        .append_record(accepted)
        .map_err(|error| error.to_string())?;

    Ok(Some(operation_id))
}
