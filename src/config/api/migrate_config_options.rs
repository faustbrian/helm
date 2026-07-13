use std::path::Path;

/// Explicit source and target format for one config-only migration.
#[derive(Clone, Copy)]
pub struct MigrateConfigOptions<'operation> {
    pub config_path: Option<&'operation Path>,
    pub project_root: Option<&'operation Path>,
    pub runtime_env: Option<&'operation str>,
    pub to: &'operation str,
}
