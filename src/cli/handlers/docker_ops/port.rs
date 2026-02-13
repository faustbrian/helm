//! cli handlers docker ops port module.
//!
//! Contains port handler used by Helm command workflows.

use anyhow::Result;

use crate::{cli, config, docker};

pub(super) fn handle_port(
    config: &config::Config,
    service: Option<&str>,
    kind: Option<config::Kind>,
    json: bool,
    private_port: Option<&str>,
) -> Result<()> {
    let selected = cli::support::selected_services(config, service, kind, None)?;
    if json {
        let mut items = Vec::new();
        for svc in selected {
            let container_name = svc.container_name()?;
            let output = docker::port_output(svc, private_port)?;
            for line in output.lines() {
                if let Some(parsed) = parse_port_line(line) {
                    items.push(serde_json::json!({
                        "service": svc.name,
                        "container": container_name,
                        "private_port": parsed.private_port,
                        "host": parsed.host,
                        "host_port": parsed.host_port,
                    }));
                }
            }
        }
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(());
    }

    for svc in selected {
        docker::port(svc, private_port)?;
    }
    Ok(())
}

struct ParsedPortLine {
    private_port: String,
    host: String,
    host_port: u16,
}

fn parse_port_line(line: &str) -> Option<ParsedPortLine> {
    let (private_port, rhs) = line.split_once(" -> ")?;
    let split_at = rhs.rfind(':')?;
    let host = rhs.get(..split_at)?.to_owned();
    let host_port = rhs.get(split_at + 1..)?.parse::<u16>().ok()?;

    Some(ParsedPortLine {
        private_port: private_port.to_owned(),
        host,
        host_port,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_port_line;

    #[test]
    fn parses_standard_docker_port_line() {
        let parsed = parse_port_line("80/tcp -> 0.0.0.0:8080").expect("parse");
        assert_eq!(parsed.private_port, "80/tcp");
        assert_eq!(parsed.host, "0.0.0.0");
        assert_eq!(parsed.host_port, 8080);
    }
}
