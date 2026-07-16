use super::{
    IpcError, IpcRequest, IpcResponse, decode_request_frame, encode_frame, frame::MAX_FRAME_BYTES,
};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::time::Duration;

const IPC_IO_TIMEOUT: Duration = Duration::from_secs(5);

/// A user-only Unix socket owned for the daemon process lifetime.
#[derive(Debug)]
pub(crate) struct UnixIpcListener {
    path: PathBuf,
    listener: UnixListener,
}

impl UnixIpcListener {
    /// Binds a new endpoint without deleting or replacing an existing path.
    pub(crate) fn bind(path: &Path) -> Result<Self, IpcError> {
        let listener = UnixListener::bind(path).map_err(|source| IpcError::EndpointIo {
            path: path.to_path_buf(),
            source,
        })?;

        if let Err(source) = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        {
            drop(listener);
            let _cleanup_result = std::fs::remove_file(path);

            return Err(IpcError::EndpointIo {
                path: path.to_path_buf(),
                source,
            });
        }

        Ok(Self {
            path: path.to_path_buf(),
            listener,
        })
    }

    /// Accepts, validates, dispatches, and answers one bounded request.
    #[cfg(test)]
    pub(crate) fn serve_next<Handler>(&self, handler: Handler) -> Result<IpcRequest, IpcError>
    where
        Handler: FnOnce(&IpcRequest) -> IpcResponse,
    {
        let (mut stream, _peer) = self
            .listener
            .accept()
            .map_err(|source| self.endpoint_error(source))?;
        self.serve_stream(&mut stream, handler)
    }

    /// Enables nonblocking accept for a daemon event loop.
    pub(crate) fn set_nonblocking(&self, nonblocking: bool) -> Result<(), IpcError> {
        self.listener
            .set_nonblocking(nonblocking)
            .map_err(|source| self.endpoint_error(source))
    }

    /// Serves one pending request without waiting when no client is ready.
    pub(crate) fn try_serve_next<Handler>(
        &self,
        handler: Handler,
    ) -> Result<Option<IpcRequest>, IpcError>
    where
        Handler: FnOnce(&IpcRequest) -> IpcResponse,
    {
        match self.listener.accept() {
            Ok((mut stream, _peer)) => self.serve_stream(&mut stream, handler).map(Some),
            Err(source) if source.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(source) => Err(self.endpoint_error(source)),
        }
    }

    fn serve_stream<Handler>(
        &self,
        stream: &mut std::os::unix::net::UnixStream,
        handler: Handler,
    ) -> Result<IpcRequest, IpcError>
    where
        Handler: FnOnce(&IpcRequest) -> IpcResponse,
    {
        stream
            .set_read_timeout(Some(IPC_IO_TIMEOUT))
            .map_err(|source| self.endpoint_error(source))?;
        stream
            .set_write_timeout(Some(IPC_IO_TIMEOUT))
            .map_err(|source| self.endpoint_error(source))?;
        let mut frame = Vec::new();
        {
            let bounded = (&mut *stream).take((MAX_FRAME_BYTES + 1) as u64);
            BufReader::new(bounded)
                .read_until(b'\n', &mut frame)
                .map_err(|source| self.endpoint_error(source))?;
        }
        let request = decode_request_frame(&frame)?;
        let response = handler(&request);
        let response_frame = encode_frame(&response)?;
        if let Err(source) = stream.write_all(&response_frame) {
            if !peer_disconnected(&source) {
                return Err(self.endpoint_error(source));
            }

            return Ok(request);
        }
        if let Err(source) = stream.flush()
            && !peer_disconnected(&source)
        {
            return Err(self.endpoint_error(source));
        }

        Ok(request)
    }

    fn endpoint_error(&self, source: std::io::Error) -> IpcError {
        IpcError::EndpointIo {
            path: self.path.clone(),
            source,
        }
    }
}

fn peer_disconnected(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::NotConnected
    )
}

impl Drop for UnixIpcListener {
    fn drop(&mut self) {
        let _cleanup_result = std::fs::remove_file(&self.path);
    }
}
