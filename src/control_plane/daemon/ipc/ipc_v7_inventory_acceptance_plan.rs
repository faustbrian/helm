use super::IpcV7ProjectInventory;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const CONFIRMATION_PURPOSE: &[u8] = b"stackctl:v8:accept-v7-inventory\0";

/// Exact secret-free legacy evidence and purpose-bound acceptance token.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IpcV7InventoryAcceptancePlan {
    inventory: IpcV7ProjectInventory,
    evidence_revision: String,
    confirmation_token: Option<String>,
}

impl IpcV7InventoryAcceptancePlan {
    pub(crate) fn new(inventory: IpcV7ProjectInventory) -> Result<Self, String> {
        let inventory_json = serde_json::to_string(&inventory)
            .map_err(|error| format!("failed to serialize legacy inventory evidence: {error}"))?;
        let evidence_revision = hex::encode(Sha256::digest(inventory_json.as_bytes()));
        let confirmation_token = inventory.ready_for_automatic_migration().then(|| {
            let mut digest = Sha256::new();
            digest.update(CONFIRMATION_PURPOSE);
            digest.update(evidence_revision.as_bytes());
            hex::encode(digest.finalize())
        });

        Ok(Self {
            inventory,
            evidence_revision,
            confirmation_token,
        })
    }

    pub(crate) const fn inventory(&self) -> &IpcV7ProjectInventory {
        &self.inventory
    }

    pub(crate) fn inventory_json(&self) -> Result<String, String> {
        serde_json::to_string(&self.inventory)
            .map_err(|error| format!("failed to serialize legacy inventory evidence: {error}"))
    }

    pub(crate) fn evidence_revision(&self) -> &str {
        &self.evidence_revision
    }

    pub(crate) fn confirmation_token(&self) -> Option<&str> {
        self.confirmation_token.as_deref()
    }

    pub(crate) fn confirms(&self, token: &str) -> bool {
        self.confirmation_token.as_deref() == Some(token)
    }
}
