use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawSwarmGit {
    pub repo: String,
    #[serde(default)]
    pub branch: Option<String>,
}
