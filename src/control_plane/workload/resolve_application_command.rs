use crate::control_plane::ServiceExecutionPlan;

const PROJECT_SOURCE_TARGET: &str = "/workspace";

/// Resolves the explicit or preset-owned command for one application runtime.
pub(crate) fn resolve_application_command(
    service: &ServiceExecutionPlan,
    internal_http_port: u16,
) -> Vec<String> {
    if let Some(command) = service.desired().command() {
        return command.to_vec();
    }

    match service.desired().preset() {
        Some("laravel" | "frankenphp") => vec![
            "frankenphp".to_owned(),
            "php-server".to_owned(),
            "--listen".to_owned(),
            format!(":{internal_http_port}"),
            "--root".to_owned(),
            format!("{PROJECT_SOURCE_TARGET}/public"),
        ],
        Some("reverb") => vec![
            "php".to_owned(),
            "artisan".to_owned(),
            "reverb:start".to_owned(),
            "--host=0.0.0.0".to_owned(),
            format!("--port={internal_http_port}"),
        ],
        _ => Vec::new(),
    }
}
