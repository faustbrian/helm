use super::{V7HostArtifactPaths, V7ProjectInventoryProvider, ipc::IpcV7ProjectInventory};
use crate::config::{LoadConfigPathOptions, load_config_with};
use crate::control_plane::engine::LegacyContainerDiscovery;
use crate::control_plane::migration::{
    V7HostArtifactDiscoveryOptions, V7ProjectInventoryRequest, inventory_v7_host_artifacts,
    inventory_v7_project,
};
use sha2::{Digest, Sha256};
use std::path::Path;

/// Bridges explicit legacy config parsing to typed Engine source discovery.
pub(crate) struct EngineV7ProjectInventoryProvider<'runtime, Engine> {
    runtime: &'runtime tokio::runtime::Runtime,
    engine: Engine,
    host_artifact_paths: Option<V7HostArtifactPaths>,
}

impl<'runtime, Engine> EngineV7ProjectInventoryProvider<'runtime, Engine> {
    pub(crate) const fn new(runtime: &'runtime tokio::runtime::Runtime, engine: Engine) -> Self {
        Self {
            runtime,
            engine,
            host_artifact_paths: None,
        }
    }

    pub(crate) fn with_host_artifact_paths(mut self, paths: V7HostArtifactPaths) -> Self {
        self.host_artifact_paths = Some(paths);
        self
    }
}

impl<Engine> V7ProjectInventoryProvider for EngineV7ProjectInventoryProvider<'_, Engine>
where
    Engine: LegacyContainerDiscovery,
{
    fn inventory(
        &mut self,
        canonical_project_path: &Path,
        maximum_config_bytes: usize,
    ) -> Result<IpcV7ProjectInventory, String> {
        let source = canonical_project_path.join(".stackctl.toml");
        let metadata = std::fs::symlink_metadata(&source).map_err(|error| {
            format!(
                "failed to inspect legacy config '{}': {error}",
                source.display()
            )
        })?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(format!(
                "legacy config '{}' must be a regular non-symlink file",
                source.display()
            ));
        }
        let source_bytes = std::fs::read(&source).map_err(|error| {
            format!(
                "failed to read legacy config '{}': {error}",
                source.display()
            )
        })?;
        if source_bytes.len() > maximum_config_bytes {
            return Err(format!(
                "legacy config '{}' exceeds the {} byte inventory limit",
                source.display(),
                maximum_config_bytes
            ));
        }
        let config = load_config_with(LoadConfigPathOptions {
            config_path: Some(&source),
            project_root: Some(canonical_project_path),
            runtime_env: None,
        })
        .map_err(|error| format!("legacy config expansion failed: {error}"))?;
        let confirmed_bytes = std::fs::read(&source).map_err(|error| {
            format!(
                "failed to re-read legacy config '{}': {error}",
                source.display()
            )
        })?;
        if source_bytes != confirmed_bytes {
            return Err(
                "legacy config changed during inventory; retry after the write completes"
                    .to_owned(),
            );
        }
        let project_id = config
            .container_prefix
            .as_deref()
            .filter(|value| !value.is_empty() && *value != "stackctl")
            .map(str::to_owned)
            .or_else(|| {
                canonical_project_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_owned)
            })
            .ok_or_else(|| "legacy project path has no UTF-8 project identity".to_owned())?;
        let source_revision = format!("sha256:{}", hex::encode(Sha256::digest(&source_bytes)));
        let inventory = self
            .runtime
            .block_on(inventory_v7_project(
                &self.engine,
                V7ProjectInventoryRequest {
                    project_id: &project_id,
                    canonical_project_path,
                    source_revision: &source_revision,
                    config: &config,
                },
            ))
            .map_err(|error| error.to_string())?;
        let paths = self
            .host_artifact_paths
            .clone()
            .map(Ok)
            .unwrap_or_else(|| V7HostArtifactPaths::for_current_user(canonical_project_path))?;
        let route_domains = inventory
            .routes()
            .iter()
            .map(|route| route.domain().to_owned())
            .collect::<Vec<_>>();
        let host_artifacts = inventory_v7_host_artifacts(V7HostArtifactDiscoveryOptions {
            environment_path: paths.environment_path(),
            hosts_path: paths.hosts_path(),
            caddy_state_path: paths.caddy_state_path(),
            caddy_ca_candidates: paths.caddy_ca_candidates(),
            route_domains: &route_domains,
            maximum_artifact_bytes: maximum_config_bytes,
        })
        .map_err(|error| error.to_string())?;
        let inventory = inventory.with_host_artifacts(host_artifacts);

        Ok(IpcV7ProjectInventory::from(&inventory))
    }
}
