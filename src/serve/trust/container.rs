//! Container-side certificate discovery/copy helpers.

use anyhow::{Context, Result};
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::config::ServiceConfig;

/// Finds the inner Caddy root cert path, warming up TLS endpoint if needed.
pub(super) fn find_container_ca_with_warmup(
    target: &ServiceConfig,
    container_name: &str,
) -> Result<Option<String>> {
    let mut cert_path = None;
    for _ in 0..3 {
        cert_path = find_container_ca_cert_path(container_name)?;
        if cert_path.is_some() {
            break;
        }
        warm_up_container_tls_endpoint(target);
        thread::sleep(Duration::from_secs(1));
    }
    Ok(cert_path)
}

/// Copies the discovered container CA certificate to a host path.
pub(super) fn copy_container_ca_cert(
    container_name: &str,
    cert_path: &str,
    output_path: &str,
) -> Result<()> {
    let copy_output = Command::new("docker")
        .args(["cp", &format!("{container_name}:{cert_path}"), output_path])
        .output()
        .context("failed to copy inner caddy root certificate from container")?;

    if !copy_output.status.success() {
        let stderr = String::from_utf8_lossy(&copy_output.stderr);
        anyhow::bail!("failed to copy container CA certificate from '{cert_path}': {stderr}");
    }

    Ok(())
}

/// Locates a likely `root.crt` path inside the running container.
fn find_container_ca_cert_path(container_name: &str) -> Result<Option<String>> {
    let probe_script = r#"
for d in /data/caddy /data /var/lib/caddy /root/.local/share/caddy /home/*/.local/share/caddy; do
  if [ -f "$d/pki/authorities/local/root.crt" ]; then
    echo "$d/pki/authorities/local/root.crt"
    exit 0
  fi
done
find / -path '*/pki/authorities/local/root.crt' 2>/dev/null | head -n 1
"#;

    let output = Command::new("docker")
        .args(["exec", container_name, "sh", "-lc", probe_script])
        .output()
        .context("failed to probe container CA certificate path")?;

    if !output.status.success() {
        return Ok(None);
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if path.is_empty() {
        return Ok(None);
    }

    Ok(Some(path))
}

/// Sends a short TLS request to trigger lazy certificate materialization.
fn warm_up_container_tls_endpoint(target: &ServiceConfig) {
    let url = format!("https://{}:{}", target.host, target.port);
    drop(
        Command::new("curl")
            .args(["-kfsSI", "--max-time", "5", &url])
            .output(),
    );
}
