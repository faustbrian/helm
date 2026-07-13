mod frame;
mod ipc_error;
mod ipc_event;
mod ipc_event_journal;
mod ipc_event_journal_error;
mod ipc_event_kind;
mod ipc_output_stream;
mod ipc_payload;
mod ipc_project_command;
mod ipc_request;
mod ipc_response;
mod ipc_result;
#[cfg(unix)]
mod send_unix_request;
#[cfg(unix)]
mod unix_ipc_listener;

pub(crate) use frame::{decode_request_frame, decode_response_frame, encode_frame};
pub(crate) use ipc_error::IpcError;
pub(crate) use ipc_event::IpcEvent;
pub(crate) use ipc_event_journal::IpcEventJournal;
pub(crate) use ipc_event_journal_error::IpcEventJournalError;
pub(crate) use ipc_event_kind::IpcEventKind;
pub(crate) use ipc_output_stream::IpcOutputStream;
pub(crate) use ipc_payload::IpcPayload;
pub(crate) use ipc_project_command::IpcProjectCommand;
pub(crate) use ipc_request::{IPC_PROTOCOL_VERSION, IpcRequest};
pub(crate) use ipc_response::{IpcDiagnostic, IpcOutcome, IpcResponse};
pub(crate) use ipc_result::IpcResult;
#[cfg(unix)]
pub(crate) use send_unix_request::send_unix_request;
#[cfg(unix)]
pub(crate) use unix_ipc_listener::UnixIpcListener;

#[cfg(test)]
mod tests;
