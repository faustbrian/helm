mod frame;
mod ipc_error;
mod ipc_payload;
mod ipc_request;
mod ipc_response;
mod ipc_result;
#[cfg(unix)]
mod unix_ipc_listener;

pub(crate) use frame::{decode_request_frame, decode_response_frame, encode_frame};
pub(crate) use ipc_error::IpcError;
pub(crate) use ipc_payload::IpcPayload;
pub(crate) use ipc_request::{IPC_PROTOCOL_VERSION, IpcRequest};
pub(crate) use ipc_response::IpcResponse;
pub(crate) use ipc_result::IpcResult;
#[cfg(unix)]
pub(crate) use unix_ipc_listener::UnixIpcListener;

#[cfg(test)]
mod tests;
