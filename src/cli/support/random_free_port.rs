//! cli support random free port module.
//!
//! Contains cli support random free port logic used by Helm command workflows.

use anyhow::{Context, Result};
use std::net::TcpListener;

pub(crate) fn random_free_port(host: &str) -> Result<u16> {
    let listener = TcpListener::bind((host, 0))
        .or_else(|_| TcpListener::bind(("127.0.0.1", 0)))
        .with_context(|| format!("failed to allocate free port for host '{host}'"))?;

    let port = listener
        .local_addr()
        .context("failed to read allocated local address")?
        .port();

    Ok(port)
}
