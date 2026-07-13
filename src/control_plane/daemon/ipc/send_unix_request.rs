use super::{
    IpcError, IpcRequest, IpcResponse, decode_response_frame, encode_frame, frame::MAX_FRAME_BYTES,
};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

/// Sends one bounded correlated request to the per-user Unix daemon.
pub(crate) fn send_unix_request(
    socket_path: &Path,
    request: &IpcRequest,
    timeout: Duration,
) -> Result<IpcResponse, IpcError> {
    if timeout.is_zero() {
        return Err(IpcError::InvalidTimeout);
    }
    let mut stream =
        UnixStream::connect(socket_path).map_err(|source| endpoint_error(socket_path, source))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|source| endpoint_error(socket_path, source))?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(|source| endpoint_error(socket_path, source))?;
    stream
        .write_all(&encode_frame(request)?)
        .map_err(|source| endpoint_error(socket_path, source))?;
    stream
        .flush()
        .map_err(|source| endpoint_error(socket_path, source))?;
    let mut frame = Vec::new();
    BufReader::new(stream.take((MAX_FRAME_BYTES + 1) as u64))
        .read_until(b'\n', &mut frame)
        .map_err(|source| endpoint_error(socket_path, source))?;
    let response = decode_response_frame(&frame)?;
    if response.request_id() != request.request_id() {
        return Err(IpcError::ResponseCorrelation {
            expected: request.request_id().to_owned(),
            found: response.request_id().to_owned(),
        });
    }

    Ok(response)
}

fn endpoint_error(path: &Path, source: std::io::Error) -> IpcError {
    IpcError::EndpointIo {
        path: path.to_path_buf(),
        source,
    }
}
