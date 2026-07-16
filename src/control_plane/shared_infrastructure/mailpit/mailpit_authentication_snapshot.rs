use super::{MailpitPlanError, MailpitProjectDefinition};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};

/// Complete deterministic SMTP authentication state for one shared Mailpit.
pub(crate) struct MailpitAuthenticationSnapshot {
    contents: Vec<u8>,
    revision: String,
    project_count: usize,
}

impl MailpitAuthenticationSnapshot {
    pub(crate) fn new(projects: Vec<MailpitProjectDefinition>) -> Result<Self, MailpitPlanError> {
        let mut by_username = BTreeMap::new();
        for project in projects {
            let username = project.username().to_owned();
            if by_username.insert(username.clone(), project).is_some() {
                return Err(MailpitPlanError::new(format!(
                    "Mailpit SMTP user '{username}' is defined more than once"
                )));
            }
        }
        let mut contents = Vec::new();
        for project in by_username.values() {
            contents.extend_from_slice(project.username().as_bytes());
            contents.push(b':');
            contents.extend_from_slice(project.password_hash().as_bytes());
            contents.push(b'\n');
        }
        let revision = format!("sha256:{}", hex::encode(Sha256::digest(&contents)));

        Ok(Self {
            contents,
            revision,
            project_count: by_username.len(),
        })
    }

    pub(crate) fn contents(&self) -> &[u8] {
        &self.contents
    }

    pub(crate) fn revision(&self) -> &str {
        &self.revision
    }

    #[cfg(test)]
    pub(crate) const fn project_count(&self) -> usize {
        self.project_count
    }
}

impl Debug for MailpitAuthenticationSnapshot {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MailpitAuthenticationSnapshot")
            .field("project_count", &self.project_count)
            .finish()
    }
}
