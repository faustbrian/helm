use super::{IpcInstallationVolumeDeletion, IpcPostgresPrunePlan};
use crate::control_plane::retention::InstallationDeletionPlan;
use serde::{Deserialize, Serialize};

/// Secret-free exact installation teardown intent returned before mutation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcInstallationDeletionPlan {
    logical_prunes: Vec<IpcPostgresPrunePlan>,
    volume_deletions: Vec<IpcInstallationVolumeDeletion>,
    confirmation_token: String,
}

impl IpcInstallationDeletionPlan {
    pub(crate) fn new(
        logical_prunes: Vec<IpcPostgresPrunePlan>,
        volume_deletions: Vec<IpcInstallationVolumeDeletion>,
        confirmation_token: String,
    ) -> Result<Self, String> {
        if confirmation_token.len() != 64
            || !confirmation_token
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(
                "installation deletion token must be 64 lowercase hexadecimal characters"
                    .to_owned(),
            );
        }

        Ok(Self {
            logical_prunes,
            volume_deletions,
            confirmation_token,
        })
    }

    pub(crate) fn logical_prunes(&self) -> &[IpcPostgresPrunePlan] {
        &self.logical_prunes
    }

    pub(crate) fn confirmation_token(&self) -> &str {
        &self.confirmation_token
    }

    pub(crate) fn volume_deletions(&self) -> &[IpcInstallationVolumeDeletion] {
        &self.volume_deletions
    }
}

impl From<&InstallationDeletionPlan> for IpcInstallationDeletionPlan {
    fn from(plan: &InstallationDeletionPlan) -> Self {
        Self {
            logical_prunes: plan
                .logical_prunes()
                .iter()
                .map(IpcPostgresPrunePlan::from)
                .collect(),
            volume_deletions: plan
                .volume_deletions()
                .iter()
                .map(IpcInstallationVolumeDeletion::from)
                .collect(),
            confirmation_token: plan.confirmation_token().to_owned(),
        }
    }
}
