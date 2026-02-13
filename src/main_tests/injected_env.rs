use super::*;

#[test]
fn project_dependency_injected_env_resolves_base_url_from_domain() -> Result<()> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let workspace = std::env::temp_dir().join(format!("helm-project-deps-env-{nonce}"));
    let api_root = workspace.join("api");
    let location_root = workspace.join("location");
    std::fs::create_dir_all(&api_root)?;
    std::fs::create_dir_all(&location_root)?;

    std::fs::write(api_root.join(".helm.toml"), "container_prefix = \"api\"\n")?;
    std::fs::write(
        location_root.join(".helm.toml"),
        r#"
container_prefix = "location"

[[service]]
preset = "laravel"
domain = "acme-location.grid"
"#,
    )?;
    std::fs::write(
        workspace.join(".helm.toml"),
        r#"
[[swarm]]
name = "api"
root = "api"
depends_on = ["location"]

[[swarm.inject_env]]
env = "LOCATION_API_BASE_URL"
from = "location"
value = ":base_url"

[[swarm]]
name = "location"
root = "location"
"#,
    )?;

    let injected = resolve_project_dependency_injected_env(&api_root)?;
    assert_eq!(
        injected.get("LOCATION_API_BASE_URL").map(String::as_str),
        Some("https://acme-location.grid")
    );

    std::fs::remove_dir_all(workspace)?;
    Ok(())
}

#[test]
fn project_dependency_injected_env_resolves_when_workspace_config_is_project_root() -> Result<()> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let api_root = std::env::temp_dir().join(format!("helm-project-root-workspace-{nonce}"));
    let location_root = api_root.join("location");
    std::fs::create_dir_all(&location_root)?;

    std::fs::write(
        api_root.join(".helm.toml"),
        r#"
container_prefix = "api"

[[swarm]]
name = "api"
root = "./"
depends_on = ["location"]

[[swarm.inject_env]]
env = "LOCATION_API_BASE_URL"
from = "location"
value = ":base_url"

[[swarm]]
name = "location"
root = "location"
"#,
    )?;
    std::fs::write(
        location_root.join(".helm.toml"),
        r#"
container_prefix = "location"

[[service]]
preset = "laravel"
domain = "acme-location.grid"
"#,
    )?;

    let injected = resolve_project_dependency_injected_env(&api_root)?;
    assert_eq!(
        injected.get("LOCATION_API_BASE_URL").map(String::as_str),
        Some("https://acme-location.grid")
    );

    std::fs::remove_dir_all(api_root)?;
    Ok(())
}

#[test]
fn resolve_injected_env_value_maps_loopback_host_for_host_token() -> Result<()> {
    let mut service = crate::config::preset_preview("laravel")?;
    service.host = "127.0.0.1".to_owned();

    let resolved = resolve_injected_env_value(":host", &service, None)?;
    assert_eq!(resolved, "host.docker.internal");
    Ok(())
}

#[test]
fn resolve_injected_env_value_prefers_runtime_port_for_port_token() -> Result<()> {
    let mut service = crate::config::preset_preview("mysql")?;
    service.port = 33060;

    let resolved = resolve_injected_env_value(":port", &service, Some(49123))?;
    assert_eq!(resolved, "49123");
    Ok(())
}

#[test]
fn resolve_injected_env_value_prefers_runtime_port_for_url_tokens() -> Result<()> {
    let mut service = crate::config::preset_preview("mysql")?;
    service.host = "127.0.0.1".to_owned();
    service.port = 33060;

    let resolved = resolve_injected_env_value(":url", &service, Some(49123))?;
    assert_eq!(resolved, "http://host.docker.internal:49123");
    Ok(())
}

#[test]
fn project_dependency_injected_env_is_empty_outside_swarm_workspace() -> Result<()> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!("helm-project-no-workspace-{nonce}"));
    std::fs::create_dir_all(&root)?;
    std::fs::write(root.join(".helm.toml"), "container_prefix = \"solo\"\n")?;

    let injected = resolve_project_dependency_injected_env(&root)?;
    assert!(injected.is_empty());

    std::fs::remove_dir_all(root)?;
    Ok(())
}
