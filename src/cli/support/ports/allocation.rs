//! cli support port allocation helpers.

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::net::TcpListener;

use super::RANDOM_PORT_FALLBACK_HOST;

pub(crate) fn random_free_port(host: &str) -> Result<u16> {
    let listener = TcpListener::bind((host, 0))
        .or_else(|_| TcpListener::bind((RANDOM_PORT_FALLBACK_HOST, 0)))
        .with_context(|| format!("failed to allocate free port for host '{host}'"))?;

    Ok(listener
        .local_addr()
        .context("failed to read allocated local address")?
        .port())
}

pub(crate) fn random_unused_port(host: &str, used_ports: &HashSet<(String, u16)>) -> Result<u16> {
    for _ in 0..100 {
        let candidate = random_free_port(host)?;
        if !used_ports.contains(&(host.to_owned(), candidate)) {
            return Ok(candidate);
        }
    }
    anyhow::bail!("failed to allocate random unused port")
}

#[cfg(test)]
pub(crate) fn is_port_available(host: &str, port: u16) -> bool {
    TcpListener::bind((host, port))
        .or_else(|_| TcpListener::bind((RANDOM_PORT_FALLBACK_HOST, port)))
        .is_ok()
}

pub(crate) fn is_port_available_strict(host: &str, port: u16) -> bool {
    TcpListener::bind((host, port)).is_ok()
}
