use super::{GatewayDocumentLoader, GatewayError, GatewayFuture};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

const ADMIN_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RESPONSE_BYTES: usize = 64 * 1_024;

/// Native HTTP client for Caddy's private host-mounted Unix admin socket.
pub(crate) struct CaddyUnixAdminClient {
    socket_path: PathBuf,
}

impl CaddyUnixAdminClient {
    pub(crate) fn new(socket_path: impl Into<PathBuf>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }
}

impl GatewayDocumentLoader for CaddyUnixAdminClient {
    fn load_document<'operation>(
        &'operation mut self,
        document: &'operation [u8],
    ) -> GatewayFuture<'operation, ()> {
        Box::pin(async move { load_document(&self.socket_path, document).await })
    }
}

async fn load_document(socket_path: &Path, document: &[u8]) -> Result<(), GatewayError> {
    let mut stream = tokio::time::timeout(ADMIN_TIMEOUT, UnixStream::connect(socket_path))
        .await
        .map_err(|_| provider_error("timed out connecting to the Caddy admin socket"))?
        .map_err(|error| {
            provider_error(format!(
                "failed to connect to Caddy admin socket '{}': {error}",
                socket_path.display()
            ))
        })?;
    let headers = format!(
        "POST /load HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        document.len()
    );

    tokio::time::timeout(ADMIN_TIMEOUT, async {
        stream.write_all(headers.as_bytes()).await?;
        stream.write_all(document).await?;
        stream.flush().await
    })
    .await
    .map_err(|_| provider_error("timed out writing the Caddy gateway document"))?
    .map_err(|error| provider_error(format!("failed to write Caddy gateway document: {error}")))?;

    let mut response = Vec::new();
    let mut limited = stream.take((MAX_RESPONSE_BYTES + 1) as u64);
    tokio::time::timeout(ADMIN_TIMEOUT, limited.read_to_end(&mut response))
        .await
        .map_err(|_| provider_error("timed out waiting for the Caddy reload response"))?
        .map_err(|error| {
            provider_error(format!("failed to read Caddy reload response: {error}"))
        })?;

    if response.len() > MAX_RESPONSE_BYTES {
        return Err(provider_error(
            "Caddy reload response exceeded the 64 KiB safety limit",
        ));
    }

    validate_response(&response)
}

fn validate_response(response: &[u8]) -> Result<(), GatewayError> {
    let response = std::str::from_utf8(response)
        .map_err(|_| provider_error("Caddy reload response was not valid UTF-8 HTTP"))?;
    let (headers, body) = response.split_once("\r\n\r\n").ok_or_else(|| {
        provider_error("Caddy reload response did not contain complete HTTP headers")
    })?;
    let status = headers
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|status| status.parse::<u16>().ok())
        .ok_or_else(|| provider_error("Caddy reload response had an invalid HTTP status"))?;

    if (200..300).contains(&status) {
        return Ok(());
    }

    let detail = body.trim();
    let detail = if detail.is_empty() {
        format!("Caddy rejected the gateway document with HTTP {status}")
    } else {
        format!("Caddy rejected the gateway document with HTTP {status}: {detail}")
    };

    Err(provider_error(detail))
}

fn provider_error(detail: impl Into<String>) -> GatewayError {
    GatewayError::Provider {
        detail: detail.into(),
    }
}
