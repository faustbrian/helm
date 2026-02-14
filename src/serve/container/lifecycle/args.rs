//! `docker run` argument assembly for serve containers.

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

use crate::config::ServiceConfig;

use super::super::super::mailhog_smtp_port;
use super::super::run_config::{resolve_volume_mapping, resolved_run_command};

const HOST_GATEWAY_ALIAS: &str = "host.docker.internal";
const HOST_GATEWAY_MAPPING: &str = "host.docker.internal:host-gateway";

/// Builds the full `docker run` argument list for a serve target.
///
/// Injected env values are added before service-defined env values so explicit
/// service env can override inferred defaults, except protected URL vars where
/// Helm inferred HTTPS should not be downgraded.
pub(super) fn build_run_args(
    target: &ServiceConfig,
    runtime_image: &str,
    project_root: &Path,
    injected_env: &HashMap<String, String>,
    inject_server_name: bool,
) -> Result<Vec<String>> {
    let mut run_args = vec![
        "run".to_owned(),
        "-d".to_owned(),
        "--name".to_owned(),
        target.container_name()?,
        "-p".to_owned(),
        format!(
            "{}:{}:{}",
            target.host,
            target.port,
            target.resolved_container_port()
        ),
    ];
    if let Some(smtp_port) = mailhog_smtp_port(target) {
        run_args.push("-p".to_owned());
        run_args.push(format!("{}:{}:1025", target.host, smtp_port));
    }
    append_host_gateway_mapping(&mut run_args, target, injected_env);

    if let Some(volumes) = &target.volumes {
        for volume in volumes {
            run_args.push("-v".to_owned());
            run_args.push(resolve_volume_mapping(volume, project_root)?);
        }
    }

    for (key, value) in injected_env {
        run_args.push("-e".to_owned());
        run_args.push(format!("{key}={value}"));
    }

    if let Some(env_vars) = &target.env {
        for (key, value) in env_vars {
            if should_block_insecure_url_override(key, value, injected_env) {
                continue;
            }
            run_args.push("-e".to_owned());
            run_args.push(format!("{key}={value}"));
        }
    }

    if inject_server_name {
        run_args.push("-e".to_owned());
        run_args.push("SERVER_NAME=:80".to_owned());
    }

    run_args.push(runtime_image.to_owned());

    if let Some(command) = resolved_run_command(target) {
        run_args.extend(command);
    }

    Ok(run_args)
}

fn should_block_insecure_url_override(
    key: &str,
    value: &str,
    injected_env: &HashMap<String, String>,
) -> bool {
    if !matches!(key, "APP_URL" | "ASSET_URL") {
        return false;
    }

    let Some(inferred_value) = injected_env.get(key) else {
        return false;
    };

    inferred_value.starts_with("https://") && !value.starts_with("https://")
}

fn append_host_gateway_mapping(
    run_args: &mut Vec<String>,
    target: &ServiceConfig,
    injected_env: &HashMap<String, String>,
) {
    let injected_requests_gateway = injected_env
        .values()
        .any(|value| value.contains(HOST_GATEWAY_ALIAS));
    let explicit_requests_gateway = target.env.as_ref().is_some_and(|values| {
        values
            .values()
            .any(|value| value.contains(HOST_GATEWAY_ALIAS))
    });

    if target.uses_host_gateway_alias() || injected_requests_gateway || explicit_requests_gateway {
        run_args.push("--add-host".to_owned());
        run_args.push(HOST_GATEWAY_MAPPING.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::build_run_args;
    use crate::config::{Driver, Kind, ServiceConfig};
    use std::collections::HashMap;
    use std::path::Path;

    fn app_service() -> ServiceConfig {
        ServiceConfig {
            name: "app".to_owned(),
            kind: Kind::App,
            driver: Driver::Frankenphp,
            image: "dunglas/frankenphp:php8.5".to_owned(),
            host: "127.0.0.1".to_owned(),
            port: 33065,
            database: None,
            username: None,
            password: None,
            bucket: None,
            access_key: None,
            secret_key: None,
            api_key: None,
            region: None,
            scheme: None,
            domain: Some("acme-api.helm".to_owned()),
            domains: None,
            container_port: Some(80),
            smtp_port: None,
            volumes: None,
            env: None,
            command: None,
            depends_on: None,
            seed_file: None,
            hook: Vec::new(),
            health_path: None,
            health_statuses: None,
            localhost_tls: false,
            octane: false,
            php_extensions: None,
            trust_container_ca: false,
            env_mapping: None,
            container_name: Some("acme-api-app".to_owned()),
            resolved_container_name: Some("acme-api-app".to_owned()),
        }
    }

    #[test]
    fn blocks_http_override_for_inferred_https_app_urls() {
        let mut service = app_service();
        service.env = Some(HashMap::from([
            ("APP_URL".to_owned(), "http://localhost".to_owned()),
            ("ASSET_URL".to_owned(), "http://localhost".to_owned()),
        ]));
        let injected = HashMap::from([
            ("APP_URL".to_owned(), "https://acme-api.helm".to_owned()),
            ("ASSET_URL".to_owned(), "https://acme-api.helm".to_owned()),
        ]);

        let args = build_run_args(&service, "runtime-image", Path::new("."), &injected, false)
            .expect("run args");
        let app_https_count = args
            .iter()
            .filter(|v| *v == "APP_URL=https://acme-api.helm")
            .count();
        let app_http_count = args
            .iter()
            .filter(|v| *v == "APP_URL=http://localhost")
            .count();
        let asset_https_count = args
            .iter()
            .filter(|v| *v == "ASSET_URL=https://acme-api.helm")
            .count();
        let asset_http_count = args
            .iter()
            .filter(|v| *v == "ASSET_URL=http://localhost")
            .count();

        assert_eq!(app_https_count, 1);
        assert_eq!(app_http_count, 0);
        assert_eq!(asset_https_count, 1);
        assert_eq!(asset_http_count, 0);
    }

    #[test]
    fn allows_https_override_for_inferred_https_app_urls() {
        let mut service = app_service();
        service.env = Some(HashMap::from([(
            "APP_URL".to_owned(),
            "https://custom.example".to_owned(),
        )]));
        let injected = HashMap::from([("APP_URL".to_owned(), "https://acme-api.helm".to_owned())]);

        let args = build_run_args(&service, "runtime-image", Path::new("."), &injected, false)
            .expect("run args");
        let override_count = args
            .iter()
            .filter(|v| *v == "APP_URL=https://custom.example")
            .count();

        assert_eq!(override_count, 1);
    }

    #[test]
    fn adds_host_gateway_mapping_when_injected_env_uses_alias() {
        let mut service = app_service();
        service.host = "10.0.0.5".to_owned();
        let injected = HashMap::from([("DB_HOST".to_owned(), "host.docker.internal".to_owned())]);

        let args = build_run_args(&service, "runtime-image", Path::new("."), &injected, false)
            .expect("run args");
        let rendered = args.join(" ");

        assert!(rendered.contains("--add-host host.docker.internal:host-gateway"));
    }
}
