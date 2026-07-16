/// Returns the deterministic private Engine network for one project.
pub(crate) fn project_network_name(global_network_name: &str, project_id: &str) -> String {
    format!("{global_network_name}-{project_id}")
}
