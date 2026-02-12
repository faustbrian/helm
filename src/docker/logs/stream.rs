use anyhow::{Context, Result};
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

pub(super) fn stream_logs_with_prefix(
    args: &[String],
    service_name: &str,
    container_name: &str,
) -> Result<()> {
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut child = Command::new("docker")
        .args(&arg_refs)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to execute docker logs command")?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture logs stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to capture logs stderr"))?;

    let out_name = service_name.to_owned();
    let out_handle = std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().map_while(std::result::Result::ok) {
            println!("[{out_name}] {line}");
        }
    });

    let err_name = service_name.to_owned();
    let err_handle = std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(std::result::Result::ok) {
            println!("[{err_name}] {line}");
        }
    });

    let status = child.wait().context("Failed to wait on docker logs")?;
    drop(out_handle.join());
    drop(err_handle.join());

    if !status.success() {
        anyhow::bail!("Failed to get logs for container '{container_name}'");
    }

    Ok(())
}
