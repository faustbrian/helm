//! Share session models.

use crate::share::ShareProvider;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ShareSession {
    pub id: String,
    pub provider: ShareProvider,
    pub service: String,
    pub local_url: String,
    pub public_url: Option<String>,
    pub pid: Option<u32>,
    pub log_path: String,
    pub command: Vec<String>,
    pub started_at_unix: u64,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ShareSessionStatus {
    pub id: String,
    pub provider: ShareProvider,
    pub service: String,
    pub local_url: String,
    pub public_url: Option<String>,
    pub pid: Option<u32>,
    pub running: bool,
    pub log_path: String,
    pub command: Vec<String>,
    pub started_at_unix: u64,
}

impl ShareSession {
    pub fn to_status(&self, running: bool) -> ShareSessionStatus {
        ShareSessionStatus {
            id: self.id.clone(),
            provider: self.provider,
            service: self.service.clone(),
            local_url: self.local_url.clone(),
            public_url: self.public_url.clone(),
            pid: self.pid,
            running,
            log_path: self.log_path.clone(),
            command: self.command.clone(),
            started_at_unix: self.started_at_unix,
        }
    }
}
