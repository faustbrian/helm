/// Returns the bundled JSON Schema for one v8 `.stackctl.yaml` file.
pub(crate) const fn project_config_schema() -> &'static str {
    include_str!("../../../schemas/stackctl-project-v8.schema.json")
}
