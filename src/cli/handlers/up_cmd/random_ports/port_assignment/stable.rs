//! Stable random-port seed and hash helpers.

use std::path::Path;

/// Returns the default value for port seed.
fn default_port_seed(workspace_root: &Path, runtime_env: Option<&str>) -> String {
    let env = runtime_env.unwrap_or("local");
    format!("{}::{env}", workspace_root.display())
}

pub(in crate::cli::handlers::up_cmd::random_ports) fn effective_port_seed(
    workspace_root: &Path,
    runtime_env: Option<&str>,
    port_seed: Option<&str>,
) -> String {
    let scoped_seed = default_port_seed(workspace_root, runtime_env);
    match port_seed {
        Some(seed) if !seed.trim().is_empty() => format!("{scoped_seed}::{seed}"),
        _ => scoped_seed,
    }
}

pub(super) fn stable_port_offset(
    seed: &str,
    service_name: &str,
    field_name: &str,
    range_size: u16,
) -> u16 {
    // FNV-1a for deterministic cross-platform hashing.
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;
    for byte in seed
        .as_bytes()
        .iter()
        .chain(b"::")
        .chain(service_name.as_bytes())
        .chain(b"::")
        .chain(field_name.as_bytes())
    {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    (hash % u64::from(range_size)) as u16
}
