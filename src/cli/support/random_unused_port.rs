//! cli support random unused port module.
//!
//! Contains cli support random unused port logic used by Helm command workflows.

use anyhow::Result;
use std::collections::HashSet;

use super::random_free_port;

/// Allocates a random free host port that is not already in `used_ports`.
pub(crate) fn random_unused_port(host: &str, used_ports: &HashSet<u16>) -> Result<u16> {
    for _ in 0..100 {
        let candidate = random_free_port(host)?;
        if !used_ports.contains(&candidate) {
            return Ok(candidate);
        }
    }
    anyhow::bail!("failed to allocate random unused port");
}

#[cfg(test)]
mod tests {
    use super::random_unused_port;
    use crate::cli;
    use std::collections::HashSet;

    #[test]
    fn random_unused_port_avoids_used_ports() {
        let used_port = cli::support::random_free_port("127.0.0.1").expect("allocate used port");
        let mut used_ports = HashSet::new();
        used_ports.insert(used_port);

        let candidate =
            random_unused_port("127.0.0.1", &used_ports).expect("allocate candidate port");
        assert_ne!(candidate, used_port);
    }
}
