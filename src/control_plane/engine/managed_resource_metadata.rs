use super::{EngineError, ResourceKind};
use std::collections::BTreeMap;
use std::path::PathBuf;

const MANAGED_LABEL: &str = "dev.stackctl.managed";
const INSTALLATION_LABEL: &str = "dev.stackctl.installation";
const KIND_LABEL: &str = "dev.stackctl.kind";
const PROJECT_LABEL: &str = "dev.stackctl.project";
const FINGERPRINT_LABEL: &str = "dev.stackctl.fingerprint";

/// Mandatory ownership metadata attached to every v8 Engine resource.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ManagedResourceMetadata {
    installation_id: String,
    kind: ResourceKind,
    project_path: Option<String>,
    compatibility_fingerprint: Option<String>,
}

impl ManagedResourceMetadata {
    /// Validates ownership metadata without lossy path conversion.
    pub(crate) fn new(
        installation_id: impl Into<String>,
        kind: ResourceKind,
        project_path: Option<PathBuf>,
        compatibility_fingerprint: Option<String>,
    ) -> Result<Self, EngineError> {
        let installation_id = installation_id.into();

        if installation_id.is_empty() {
            return Err(EngineError::InvalidRequest {
                detail: "managed resource installation ID must not be empty".to_owned(),
            });
        }

        let project_path = project_path
            .map(|path| {
                path.into_os_string()
                    .into_string()
                    .map_err(|path| EngineError::InvalidRequest {
                        detail: format!(
                            "managed resource project path '{}' is not valid UTF-8",
                            PathBuf::from(path).display()
                        ),
                    })
            })
            .transpose()?;

        Ok(Self {
            installation_id,
            kind,
            project_path,
            compatibility_fingerprint,
        })
    }

    /// Produces the complete reserved label set consumed by adapters.
    pub(crate) fn labels(&self) -> BTreeMap<String, String> {
        let mut labels = BTreeMap::from([
            (MANAGED_LABEL.to_owned(), "true".to_owned()),
            (INSTALLATION_LABEL.to_owned(), self.installation_id.clone()),
            (KIND_LABEL.to_owned(), self.kind.label().to_owned()),
        ]);

        if let Some(project_path) = &self.project_path {
            labels.insert(PROJECT_LABEL.to_owned(), project_path.clone());
        }

        if let Some(fingerprint) = &self.compatibility_fingerprint {
            labels.insert(FINGERPRINT_LABEL.to_owned(), fingerprint.clone());
        }

        labels
    }
}
