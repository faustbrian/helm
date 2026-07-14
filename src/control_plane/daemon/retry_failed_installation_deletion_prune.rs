use super::ipc::{IpcEventJournal, IpcEventKind};
use super::{PostgresPruneQueue, QueuedPostgresPrune};
use crate::control_plane::application::ControlPlane;
use crate::control_plane::state::{
    DaemonOperationRetryOptions, DaemonOperationStatus, InstallationLifecycle, StateStore,
};

/// Requeues only the exact current deletion intent after explicit reconfirmation.
pub(crate) fn retry_failed_installation_deletion_prune<Store>(
    control_plane: &mut ControlPlane<Store>,
    queue: &mut PostgresPruneQueue,
    event_journal: &mut IpcEventJournal,
    now_unix_seconds: i64,
) -> Result<Option<String>, String>
where
    Store: StateStore,
{
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
    let Some(existing) = control_plane
        .daemon_operation(&operation_id)
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    if existing.status() != DaemonOperationStatus::Failed {
        return Ok(None);
    }
    if existing.kind() != "postgres_prune" || existing.payload_json() != payload_json {
        return Err(format!(
            "installation deletion operation ID '{operation_id}' is owned by different failed intent"
        ));
    }
    queue.enqueue(queued).map_err(|error| error.to_string())?;
    let accepted_kind_json =
        serde_json::to_string(&IpcEventKind::Accepted).map_err(|error| error.to_string())?;
    let accepted = match control_plane.retry_failed_daemon_operation(DaemonOperationRetryOptions {
        operation_id: &operation_id,
        expected_kind: "postgres_prune",
        expected_payload_json: &payload_json,
        updated_at_unix_seconds: now_unix_seconds,
        accepted_kind_json: &accepted_kind_json,
        event_retention_limit: event_journal.capacity(),
    }) {
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
