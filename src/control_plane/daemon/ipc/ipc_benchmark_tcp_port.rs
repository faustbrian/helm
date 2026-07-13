use serde::{Deserialize, Serialize};

/// One published TCP binding proven to belong to an owned benchmark container.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcBenchmarkTcpPort {
    host_ip: String,
    host_port: u16,
}

impl IpcBenchmarkTcpPort {
    pub(crate) fn new(host_ip: String, host_port: u16) -> Result<Self, String> {
        if host_ip.is_empty() || host_port == 0 {
            return Err("benchmark TCP binding must include an address and port".to_owned());
        }

        Ok(Self { host_ip, host_port })
    }

    pub(crate) fn host_ip(&self) -> &str {
        &self.host_ip
    }

    pub(crate) const fn host_port(&self) -> u16 {
        self.host_port
    }
}
