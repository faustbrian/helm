//! docker health http module.
//!
//! Contains docker health http logic used by Helm command workflows.

use anyhow::{Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};

pub(super) fn http_status_code(host: &str, port: u16, path: &str) -> Result<u16> {
    let mut addrs = (host, port)
        .to_socket_addrs()
        .with_context(|| format!("failed resolving {host}:{port}"))?;
    let addr = addrs
        .next()
        .ok_or_else(|| anyhow::anyhow!("no socket addresses resolved for {host}:{port}"))?;

    let timeout = std::time::Duration::from_secs(2);
    let mut stream = TcpStream::connect_timeout(&addr, timeout)
        .with_context(|| format!("failed connecting to {host}:{port}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .context("failed setting read timeout")?;
    stream
        .set_write_timeout(Some(timeout))
        .context("failed setting write timeout")?;

    let request = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .context("failed writing health check request")?;

    let mut reader = BufReader::new(stream);
    let mut status_line = String::new();
    reader
        .read_line(&mut status_line)
        .context("failed reading health check response")?;

    let code = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("invalid HTTP status line: {status_line}"))?
        .parse::<u16>()
        .context("failed parsing HTTP status code")?;

    Ok(code)
}
