use std::collections::HashMap;

use crate::config;
use crate::output::{self, LogLevel, Persistence};

pub(super) fn check_port_conflicts(config: &config::Config) -> bool {
    let mut has_error = false;
    let mut used_ports: HashMap<(String, u16), &str> = HashMap::new();

    for svc in &config.service {
        let key = (svc.host.clone(), svc.port);
        if let Some(existing) = used_ports.insert(key.clone(), svc.name.as_str()) {
            has_error = true;
            output::event(
                "doctor",
                LogLevel::Error,
                &format!(
                    "Port conflict: {}:{} used by '{}' and '{}'",
                    key.0, key.1, existing, svc.name
                ),
                Persistence::Persistent,
            );
        }
    }

    has_error
}
