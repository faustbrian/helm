use super::{
    V7GeneratedEnvironmentArtifact, V7HostArtifactDiscoveryOptions, V7HostArtifactInventory,
    V7InventoryError, V7PublicFileArtifact,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::UNIX_EPOCH;

/// Inventories bounded, regular legacy host files without retaining env values.
pub(crate) fn inventory_v7_host_artifacts(
    options: V7HostArtifactDiscoveryOptions<'_>,
) -> Result<V7HostArtifactInventory, V7InventoryError> {
    if options.maximum_artifact_bytes == 0 {
        return Err(error(
            "legacy artifact byte limit must be greater than zero",
        ));
    }
    let route_domains = options
        .route_domains
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let generated_environment =
        generated_environment(options.environment_path, options.maximum_artifact_bytes)?;
    let hosts = read_regular(options.hosts_path, options.maximum_artifact_bytes, true)?
        .ok_or_else(|| error("required legacy hosts file disappeared during inventory"))?;
    let hosts_domains = hosts_domains(&hosts.bytes, &route_domains)?;
    let caddy_routes = caddy_routes(
        options.caddy_state_path,
        options.maximum_artifact_bytes,
        &route_domains,
    )?;
    let mut caddy_ca_certificates = options
        .caddy_ca_candidates
        .iter()
        .map(|path| public_artifact(path, options.maximum_artifact_bytes))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    caddy_ca_certificates.sort_by(|left, right| left.path().cmp(right.path()));

    Ok(V7HostArtifactInventory::new(
        generated_environment,
        options.hosts_path.to_path_buf(),
        hosts_domains,
        options.caddy_state_path.to_path_buf(),
        caddy_routes,
        caddy_ca_certificates,
    ))
}

fn generated_environment(
    path: &Path,
    maximum_bytes: usize,
) -> Result<Option<V7GeneratedEnvironmentArtifact>, V7InventoryError> {
    let Some(artifact) = read_regular(path, maximum_bytes, false)? else {
        return Ok(None);
    };
    let modified_at_unix_seconds = artifact
        .modified_at
        .duration_since(UNIX_EPOCH)
        .map_err(|_| {
            error(format!(
                "legacy environment '{}' predates the Unix epoch",
                path.display()
            ))
        })?
        .as_secs();
    let modified_at_unix_seconds = i64::try_from(modified_at_unix_seconds).map_err(|_| {
        error(format!(
            "legacy environment '{}' has an unsupported modification time",
            path.display()
        ))
    })?;
    let contents = std::str::from_utf8(&artifact.bytes).map_err(|_| {
        error(format!(
            "legacy environment '{}' is not valid UTF-8",
            path.display()
        ))
    })?;
    let keys = environment_keys(contents);

    Ok(Some(V7GeneratedEnvironmentArtifact::new(
        path.to_path_buf(),
        artifact.size_bytes,
        modified_at_unix_seconds,
        keys,
    )))
}

pub(super) fn environment_keys(contents: &str) -> Vec<String> {
    contents
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let line = line.strip_prefix("export ").unwrap_or(line);
            let (key, _) = line.split_once('=')?;
            crate::control_plane::is_valid_environment_variable_key(key).then(|| key.to_owned())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn hosts_domains(
    bytes: &[u8],
    route_domains: &BTreeSet<String>,
) -> Result<Vec<String>, V7InventoryError> {
    let contents =
        std::str::from_utf8(bytes).map_err(|_| error("legacy hosts file is not valid UTF-8"))?;
    let present = contents
        .lines()
        .filter_map(|line| line.split('#').next())
        .flat_map(str::split_whitespace)
        .filter(|entry| route_domains.contains(*entry))
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();

    Ok(present.into_iter().collect())
}

fn caddy_routes(
    path: &Path,
    maximum_bytes: usize,
    route_domains: &BTreeSet<String>,
) -> Result<BTreeMap<String, String>, V7InventoryError> {
    let Some(artifact) = read_regular(path, maximum_bytes, false)? else {
        return Ok(BTreeMap::new());
    };
    let contents = std::str::from_utf8(&artifact.bytes).map_err(|_| {
        error(format!(
            "legacy Caddy state '{}' is not valid UTF-8",
            path.display()
        ))
    })?;
    let state = toml::from_str::<LegacyCaddyState>(contents).map_err(|source| {
        error(format!(
            "failed to parse legacy Caddy state '{}': {source}",
            path.display()
        ))
    })?;

    Ok(state
        .routes
        .into_iter()
        .filter(|(domain, _)| route_domains.contains(domain))
        .collect())
}

fn public_artifact(
    path: &Path,
    maximum_bytes: usize,
) -> Result<Option<V7PublicFileArtifact>, V7InventoryError> {
    let Some(artifact) = read_regular(path, maximum_bytes, false)? else {
        return Ok(None);
    };

    Ok(Some(V7PublicFileArtifact::new(
        path.to_path_buf(),
        format!("sha256:{}", hex::encode(Sha256::digest(&artifact.bytes))),
        artifact.size_bytes,
    )))
}

pub(super) fn read_regular(
    path: &Path,
    maximum_bytes: usize,
    required: bool,
) -> Result<Option<ReadArtifact>, V7InventoryError> {
    let initial_metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound && !required => {
            return Ok(None);
        }
        Err(source) => {
            return Err(error(format!(
                "failed to inspect legacy artifact '{}': {source}",
                path.display()
            )));
        }
    };
    if !initial_metadata.is_file() || initial_metadata.file_type().is_symlink() {
        return Err(error(format!(
            "legacy artifact '{}' must be a regular non-symlink file",
            path.display()
        )));
    }
    let maximum_bytes = u64::try_from(maximum_bytes)
        .map_err(|_| error("legacy artifact byte limit exceeds filesystem limits"))?;
    if initial_metadata.len() > maximum_bytes {
        return Err(error(format!(
            "legacy artifact '{}' exceeds the {} byte limit",
            path.display(),
            maximum_bytes
        )));
    }
    let initial_modified = initial_metadata.modified().map_err(|source| {
        error(format!(
            "failed to inspect modification time for '{}': {source}",
            path.display()
        ))
    })?;
    let bytes = std::fs::read(path).map_err(|source| {
        error(format!(
            "failed to read legacy artifact '{}': {source}",
            path.display()
        ))
    })?;
    let confirmed_bytes = std::fs::read(path).map_err(|source| {
        error(format!(
            "failed to re-read legacy artifact '{}': {source}",
            path.display()
        ))
    })?;
    let confirmed_metadata = std::fs::symlink_metadata(path).map_err(|source| {
        error(format!(
            "failed to re-inspect legacy artifact '{}': {source}",
            path.display()
        ))
    })?;
    let confirmed_modified = confirmed_metadata.modified().map_err(|source| {
        error(format!(
            "failed to inspect modification time for '{}': {source}",
            path.display()
        ))
    })?;
    let observed_size = u64::try_from(bytes.len())
        .map_err(|_| error(format!("legacy artifact '{}' is too large", path.display())))?;
    if !confirmed_metadata.is_file()
        || confirmed_metadata.file_type().is_symlink()
        || bytes != confirmed_bytes
        || initial_metadata.len() != confirmed_metadata.len()
        || initial_modified != confirmed_modified
        || confirmed_metadata.len() > maximum_bytes
        || observed_size > maximum_bytes
    {
        return Err(error(format!(
            "legacy artifact '{}' changed during inventory; retry after the write completes",
            path.display()
        )));
    }

    Ok(Some(ReadArtifact {
        bytes,
        size_bytes: confirmed_metadata.len(),
        modified_at: confirmed_modified,
    }))
}

fn error(detail: impl Into<String>) -> V7InventoryError {
    V7InventoryError::new(detail)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyCaddyState {
    routes: BTreeMap<String, String>,
}

pub(super) struct ReadArtifact {
    pub(super) bytes: Vec<u8>,
    pub(super) size_bytes: u64,
    pub(super) modified_at: std::time::SystemTime,
}
