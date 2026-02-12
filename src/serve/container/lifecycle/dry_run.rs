use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

pub(super) fn print_dry_run_container_start(
    target: &ServiceConfig,
    container_name: &str,
    recreate: bool,
    runtime_image: &str,
    smtp_port: Option<u16>,
) {
    if recreate {
        output::event(
            &target.name,
            LogLevel::Info,
            &format!("[dry-run] docker rm -f {container_name}"),
            Persistence::Transient,
        );
    }

    let mut mapping = format!(
        "-p {}:{}:{}",
        target.host,
        target.port,
        target.resolved_container_port()
    );
    if let Some(smtp_port) = smtp_port {
        mapping.push_str(&format!(" -p {}:{}:1025", target.host, smtp_port));
    }

    output::event(
        &target.name,
        LogLevel::Info,
        &format!("[dry-run] docker run -d --name {container_name} {mapping} {runtime_image}"),
        Persistence::Transient,
    );
}
