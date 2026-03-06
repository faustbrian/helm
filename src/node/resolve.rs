use anyhow::{Context, Result};
use std::path::Path;

use super::{
    NodeToolchain, PackageManager, VersionManager, detect_node_package_manager, detect_node_version,
};

pub(crate) struct ResolveNodeRuntimeOptions<'a> {
    pub(crate) configured: Option<&'a NodeToolchain>,
    pub(crate) workspace_root: &'a Path,
    pub(crate) package_manager: Option<PackageManager>,
    pub(crate) version_manager: Option<VersionManager>,
    pub(crate) node_version: Option<&'a str>,
    pub(crate) require_package_manager: bool,
}

#[derive(Debug)]
pub(crate) struct ResolvedNodeRuntime {
    pub(crate) package_manager: Option<PackageManager>,
    pub(crate) version_manager: VersionManager,
    pub(crate) node_version: Option<String>,
}

pub(crate) fn resolve_node_runtime(
    options: ResolveNodeRuntimeOptions<'_>,
) -> Result<ResolvedNodeRuntime> {
    let package_manager = options
        .package_manager
        .or_else(|| options.configured.and_then(|config| config.package_manager))
        .or_else(|| detect_node_package_manager(options.workspace_root));
    let version_manager = options
        .version_manager
        .or_else(|| options.configured.and_then(|config| config.version_manager))
        .unwrap_or(VersionManager::System);
    let node_version = options
        .node_version
        .map(str::to_owned)
        .or_else(|| options.configured.and_then(|config| config.version.clone()))
        .or_else(|| detect_node_version(options.workspace_root));

    if options.require_package_manager {
        package_manager.context(
            "could not infer JS package manager; pass --package-manager <bun|npm|pnpm|yarn> or set [service.node].package_manager",
        )?;
    }

    if version_manager != VersionManager::System && node_version.is_none() {
        anyhow::bail!(
            "node version manager '{}' requires a Node version; pass --node-version or set [service.node].version",
            version_manager.as_str()
        );
    }

    Ok(ResolvedNodeRuntime {
        package_manager,
        version_manager,
        node_version,
    })
}

#[cfg(test)]
mod tests {
    use super::{ResolveNodeRuntimeOptions, resolve_node_runtime};
    use crate::node::{NodeToolchain, PackageManager, VersionManager};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_root(prefix: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        drop(fs::remove_dir_all(&root));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    #[test]
    fn resolve_node_runtime_reads_version_from_nvmrc() {
        let root = temp_root("helm-node-runtime-nvmrc");
        fs::write(root.join(".nvmrc"), "22\n").expect("write nvmrc");

        let runtime = resolve_node_runtime(ResolveNodeRuntimeOptions {
            configured: Some(&NodeToolchain {
                package_manager: Some(PackageManager::Npm),
                version_manager: Some(VersionManager::Fnm),
                version: None,
            }),
            workspace_root: &root,
            package_manager: None,
            version_manager: None,
            node_version: None,
            require_package_manager: true,
        })
        .expect("resolve runtime");

        assert_eq!(runtime.node_version.as_deref(), Some("22"));
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn resolve_node_runtime_requires_version_for_non_system_manager() {
        let root = temp_root("helm-node-runtime-version-required");

        let error = resolve_node_runtime(ResolveNodeRuntimeOptions {
            configured: Some(&NodeToolchain {
                package_manager: Some(PackageManager::Npm),
                version_manager: Some(VersionManager::Volta),
                version: None,
            }),
            workspace_root: &root,
            package_manager: None,
            version_manager: None,
            node_version: None,
            require_package_manager: true,
        })
        .expect_err("missing version should fail");

        assert!(error.to_string().contains("requires a Node version"));
        fs::remove_dir_all(root).expect("cleanup");
    }
}
