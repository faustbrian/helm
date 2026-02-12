use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct CaddyState {
    pub(crate) routes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CaddyPorts {
    pub(crate) http: u16,
    pub(crate) https: u16,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct DerivedImageLock {
    pub(crate) entries: BTreeMap<String, String>,
}

/// Result of PHP extension verification for a serve target.
#[derive(Debug, Clone)]
pub struct PhpExtensionCheck {
    pub target: String,
    pub image: String,
    pub missing: Vec<String>,
}
