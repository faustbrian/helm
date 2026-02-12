use anyhow::Result;
use std::collections::HashMap;
use std::collections::HashSet;
use std::net::TcpListener;
use std::path::{Path, PathBuf};

use crate::cli::args::PortStrategyArg;
use crate::output::{self, LogLevel, Persistence};
use crate::{cli, config, docker, env, serve};

#[allow(clippy::too_many_arguments)]
pub(super) fn run_random_ports_up(
    config: &mut config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    profile: Option<&str>,
    healthy: bool,
    timeout: u64,
    pull_policy: docker::PullPolicy,
    recreate: bool,
    force_random_ports: bool,
    port_strategy: PortStrategyArg,
    port_seed: Option<&str>,
    persist_ports: bool,
    write_env: bool,
    _parallel: usize,
    quiet: bool,
    runtime_env: Option<&str>,
    workspace_root: &Path,
    project_dependency_env: &HashMap<String, String>,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
    config_path_buf: &Option<PathBuf>,
    project_root_buf: &Option<PathBuf>,
) -> Result<()> {
    let (explicit_port_services, explicit_smtp_services) =
        explicit_port_service_names(config, config_path, project_root)?;
    let mut runtime_config = config.clone();
    let env_path = if write_env {
        Some(cli::support::default_env_path(
            config_path_buf,
            project_root_buf,
            &None,
            runtime_env,
        )?)
    } else {
        None
    };
    let selected_refs: Vec<&config::ServiceConfig> = if let Some(profile_name) = profile {
        cli::support::resolve_profile_targets(config, profile_name)?
    } else {
        cli::support::selected_services(config, service, kind, None)?
    };
    let selected: Vec<config::ServiceConfig> = selected_refs.into_iter().cloned().collect();
    let mut used_ports: HashSet<u16> = runtime_config.service.iter().map(|svc| svc.port).collect();
    let seed = effective_port_seed(workspace_root, runtime_env, port_seed);

    for mut runtime in selected {
        let uses_random_port =
            should_randomize_port(&explicit_port_services, &runtime.name, force_random_ports);
        if uses_random_port {
            runtime.port =
                assign_runtime_port(&runtime, port_strategy, &seed, &mut used_ports, "port")?;
        }
        if runtime.driver == config::Driver::Mailhog
            && should_randomize_port(&explicit_smtp_services, &runtime.name, force_random_ports)
        {
            runtime.smtp_port = Some(assign_runtime_port(
                &runtime,
                port_strategy,
                &seed,
                &mut used_ports,
                "smtp_port",
            )?);
        }
        used_ports.insert(runtime.port);
        if let Some(smtp_port) = runtime.smtp_port {
            used_ports.insert(smtp_port);
        }
        apply_runtime_binding(&mut runtime_config, &runtime)?;
        if !quiet {
            output::event(
                &runtime.name,
                LogLevel::Info,
                &format!(
                    "Starting service on port {} ({})",
                    runtime.port,
                    if uses_random_port {
                        "random"
                    } else {
                        "explicit"
                    }
                ),
                Persistence::Persistent,
            );
        }
        if runtime.kind == config::Kind::App {
            let mut injected_env = env::inferred_app_env(&runtime_config);
            injected_env.extend(project_dependency_env.clone());
            serve::run(
                &runtime,
                recreate,
                runtime.trust_container_ca,
                true,
                workspace_root,
                &injected_env,
                true,
            )?;
            if healthy {
                serve::wait_until_http_healthy(&runtime, timeout, 2, None)?;
            }
        } else {
            docker::up(
                &runtime,
                docker::UpOptions {
                    pull: pull_policy,
                    recreate,
                },
            )?;
            if healthy {
                docker::wait_until_healthy(&runtime, timeout, 2, None)?;
            }
        }
        if let Some(path) = env_path.as_deref() {
            env::update_env(&runtime, path, true)?;
        }
        if persist_ports {
            config::update_service_port(config, &runtime.name, runtime.port)?;
        }
    }

    if persist_ports {
        let path = config::save_config_with(config, config_path, project_root)?;
        if !quiet {
            output::event(
                "up",
                LogLevel::Success,
                &format!("Persisted random ports to {}", path.display()),
                Persistence::Persistent,
            );
        }
    }

    Ok(())
}

fn explicit_port_service_names(
    config: &config::Config,
    config_path: Option<&Path>,
    project_root: Option<&Path>,
) -> Result<(HashSet<String>, HashSet<String>)> {
    let raw_config = config::load_raw_config_with(config_path, project_root)?;
    let mut explicit_port_services = HashSet::new();
    let mut explicit_smtp_services = HashSet::new();

    for (raw_service, service) in raw_config.service.iter().zip(config.service.iter()) {
        if raw_service.port.is_some() {
            explicit_port_services.insert(service.name.clone());
        }
        if raw_service.smtp_port.is_some() {
            explicit_smtp_services.insert(service.name.clone());
        }
    }

    Ok((explicit_port_services, explicit_smtp_services))
}

fn should_randomize_port(
    explicit_port_services: &HashSet<String>,
    service_name: &str,
    force_random_ports: bool,
) -> bool {
    force_random_ports || !explicit_port_services.contains(service_name)
}

fn assign_runtime_port(
    service: &config::ServiceConfig,
    strategy: PortStrategyArg,
    seed: &str,
    used_ports: &mut HashSet<u16>,
    field_name: &str,
) -> Result<u16> {
    match strategy {
        PortStrategyArg::Random => assign_random_port(service, used_ports),
        PortStrategyArg::Stable => assign_stable_port(service, seed, used_ports, field_name),
    }
}

fn assign_random_port(service: &config::ServiceConfig, used_ports: &HashSet<u16>) -> Result<u16> {
    for _ in 0..100 {
        let candidate = cli::support::random_free_port(&service.host)?;
        if !used_ports.contains(&candidate) {
            return Ok(candidate);
        }
    }
    anyhow::bail!(
        "failed to allocate non-conflicting random port for '{}'",
        service.name
    )
}

fn assign_stable_port(
    service: &config::ServiceConfig,
    seed: &str,
    used_ports: &HashSet<u16>,
    field_name: &str,
) -> Result<u16> {
    const RANGE_START: u16 = 20_000;
    const RANGE_SIZE: u16 = 40_000;
    let base = stable_port_offset(seed, &service.name, field_name, RANGE_SIZE);

    for offset in 0..RANGE_SIZE {
        let candidate = RANGE_START + ((base + offset) % RANGE_SIZE);
        if used_ports.contains(&candidate) {
            continue;
        }
        if is_port_available(&service.host, candidate) {
            return Ok(candidate);
        }
    }

    anyhow::bail!(
        "failed to allocate deterministic port for '{}' using stable strategy",
        service.name
    )
}

fn is_port_available(host: &str, port: u16) -> bool {
    TcpListener::bind((host, port))
        .or_else(|_| TcpListener::bind(("127.0.0.1", port)))
        .is_ok()
}

fn default_port_seed(workspace_root: &Path, runtime_env: Option<&str>) -> String {
    let env = runtime_env.unwrap_or("local");
    format!("{}::{env}", workspace_root.display())
}

fn effective_port_seed(
    workspace_root: &Path,
    runtime_env: Option<&str>,
    port_seed: Option<&str>,
) -> String {
    let scoped_seed = default_port_seed(workspace_root, runtime_env);
    match port_seed {
        Some(seed) if !seed.trim().is_empty() => format!("{scoped_seed}::{seed}"),
        _ => scoped_seed,
    }
}

fn stable_port_offset(seed: &str, service_name: &str, field_name: &str, range_size: u16) -> u16 {
    // FNV-1a for deterministic cross-platform hashing.
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;
    for byte in seed
        .as_bytes()
        .iter()
        .chain(b"::")
        .chain(service_name.as_bytes())
        .chain(b"::")
        .chain(field_name.as_bytes())
    {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    (hash % u64::from(range_size)) as u16
}

fn apply_runtime_binding(
    runtime_config: &mut config::Config,
    runtime_service: &config::ServiceConfig,
) -> Result<()> {
    let Some(existing) = runtime_config
        .service
        .iter_mut()
        .find(|service| service.name == runtime_service.name)
    else {
        anyhow::bail!(
            "service '{}' not found while applying runtime binding",
            runtime_service.name
        );
    };

    existing.host = runtime_service.host.clone();
    existing.port = runtime_service.port;
    existing.smtp_port = runtime_service.smtp_port;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_randomize_port_skips_explicit_service() {
        let explicit = HashSet::from([String::from("db")]);
        assert!(!should_randomize_port(&explicit, "db", false));
        assert!(should_randomize_port(&explicit, "cache", false));
        assert!(should_randomize_port(&explicit, "db", true));
    }

    #[test]
    fn stable_port_offset_is_deterministic() {
        let first = stable_port_offset("seed", "db", "port", 40_000);
        let second = stable_port_offset("seed", "db", "port", 40_000);
        assert_eq!(first, second);
    }

    #[test]
    fn stable_port_offset_changes_with_seed_scope() {
        let left = stable_port_offset("/ws/a::local::shared", "db", "port", 40_000);
        let right = stable_port_offset("/ws/b::local::shared", "db", "port", 40_000);
        assert_ne!(left, right);
    }

    #[test]
    fn shared_user_seed_is_scoped_by_workspace() {
        let seed_a = effective_port_seed(Path::new("/tmp/ws-a"), Some("local"), Some("shared"));
        let seed_b = effective_port_seed(Path::new("/tmp/ws-b"), Some("local"), Some("shared"));

        let offset_a = stable_port_offset(&seed_a, "db", "port", 40_000);
        let offset_b = stable_port_offset(&seed_b, "db", "port", 40_000);

        assert_ne!(offset_a, offset_b);
    }
}
