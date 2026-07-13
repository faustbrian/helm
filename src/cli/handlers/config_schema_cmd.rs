use anyhow::Result;

/// Prints the embedded v8 project schema without reading local project state.
pub(crate) fn handle_config_schema() -> Result<()> {
    print!("{}", crate::control_plane::project_config_schema());
    Ok(())
}
