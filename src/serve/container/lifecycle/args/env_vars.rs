//! Environment and host-gateway argument helpers for serve run args.

use std::collections::HashMap;

use crate::config::ServiceConfig;

pub(super) fn append_env_args(
    run_args: &mut Vec<String>,
    target: &ServiceConfig,
    injected_env: &HashMap<String, String>,
    inject_server_name: bool,
) {
    for (key, value) in injected_env {
        append_env_binding(run_args, key, value);
    }

    if let Some(env_vars) = &target.env {
        for (key, value) in env_vars {
            if should_block_insecure_url_override(key, value, injected_env) {
                continue;
            }
            append_env_binding(run_args, key, value);
        }
    }

    if inject_server_name {
        append_env_binding(run_args, "SERVER_NAME", ":80");
    }
}

pub(super) fn append_host_gateway_mapping(
    run_args: &mut Vec<String>,
    target: &ServiceConfig,
    injected_env: &HashMap<String, String>,
) {
    let host_gateway_alias = crate::docker::host_gateway_alias();
    let injected_requests_gateway = injected_env
        .values()
        .any(|value| value.contains(host_gateway_alias));
    let explicit_requests_gateway = target.env.as_ref().is_some_and(|values| {
        values
            .values()
            .any(|value| value.contains(host_gateway_alias))
    });

    if target.uses_host_gateway_alias() || injected_requests_gateway || explicit_requests_gateway {
        if let Some(mapping) = crate::docker::host_gateway_mapping() {
            run_args.push("--add-host".to_owned());
            run_args.push(mapping.to_owned());
        }
    }
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

fn append_env_binding(run_args: &mut Vec<String>, key: &str, value: &str) {
    run_args.push("-e".to_owned());
    run_args.push(format!("{key}={value}"));
}
