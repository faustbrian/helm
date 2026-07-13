use super::{IPC_PROTOCOL_VERSION, IpcError, IpcRequest, IpcResponse};
use serde::Serialize;
use serde::de::DeserializeOwned;

pub(super) const MAX_FRAME_BYTES: usize = 1_048_576;

/// Encodes one typed value as a newline-delimited JSON frame.
pub(crate) fn encode_frame<T>(value: &T) -> Result<Vec<u8>, IpcError>
where
    T: Serialize,
{
    let mut frame = serde_json::to_vec(value)?;
    frame.push(b'\n');

    if frame.len() > MAX_FRAME_BYTES {
        return Err(IpcError::FrameTooLarge {
            actual: frame.len(),
            maximum: MAX_FRAME_BYTES,
        });
    }

    Ok(frame)
}

/// Strictly decodes and validates one complete request frame.
pub(crate) fn decode_request_frame(frame: &[u8]) -> Result<IpcRequest, IpcError> {
    let request = decode_frame::<IpcRequest>(frame)?;

    validate_protocol(request.protocol_version())?;

    if request.request_id().is_empty() {
        return Err(IpcError::EmptyRequestId);
    }

    Ok(request)
}

/// Strictly decodes and validates one complete response frame.
pub(crate) fn decode_response_frame(frame: &[u8]) -> Result<IpcResponse, IpcError> {
    let response = decode_frame::<IpcResponse>(frame)?;

    validate_protocol(response.protocol_version())?;

    if response.request_id().is_empty() {
        return Err(IpcError::EmptyRequestId);
    }

    Ok(response)
}

fn decode_frame<T>(frame: &[u8]) -> Result<T, IpcError>
where
    T: DeserializeOwned,
{
    if frame.len() > MAX_FRAME_BYTES {
        return Err(IpcError::FrameTooLarge {
            actual: frame.len(),
            maximum: MAX_FRAME_BYTES,
        });
    }

    let Some(body) = frame.strip_suffix(b"\n") else {
        return Err(IpcError::InvalidFrame);
    };

    if body.contains(&b'\n') {
        return Err(IpcError::InvalidFrame);
    }

    serde_json::from_slice(body).map_err(Into::into)
}

fn validate_protocol(found: u16) -> Result<(), IpcError> {
    if found != IPC_PROTOCOL_VERSION {
        return Err(IpcError::UnsupportedProtocol {
            found,
            expected: IPC_PROTOCOL_VERSION,
        });
    }

    Ok(())
}
