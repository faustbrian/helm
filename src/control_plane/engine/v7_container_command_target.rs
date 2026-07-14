use super::{ContainerId, EngineError};

/// Exact accepted v7 identity reverified before an in-container command starts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct V7ContainerCommandTarget {
    container_id: ContainerId,
    container_name: String,
    service_id: String,
    kind: String,
}

impl V7ContainerCommandTarget {
    pub(crate) fn new(
        container_id: ContainerId,
        container_name: impl Into<String>,
        service_id: impl Into<String>,
        kind: impl Into<String>,
    ) -> Result<Self, EngineError> {
        let container_name = container_name.into();
        let service_id = service_id.into();
        let kind = kind.into();
        if container_id.as_str().is_empty()
            || container_id.as_str().contains('\0')
            || container_name.is_empty()
            || container_name.contains('\0')
            || service_id.is_empty()
            || service_id.contains('\0')
            || kind.is_empty()
            || kind.contains('\0')
        {
            return Err(EngineError::InvalidRequest {
                detail: "v7 command target requires exact non-empty legacy identity".to_owned(),
            });
        }

        Ok(Self {
            container_id,
            container_name,
            service_id,
            kind,
        })
    }

    pub(crate) const fn container_id(&self) -> &ContainerId {
        &self.container_id
    }

    pub(crate) fn container_name(&self) -> &str {
        &self.container_name
    }

    pub(crate) fn service_id(&self) -> &str {
        &self.service_id
    }

    pub(crate) fn kind(&self) -> &str {
        &self.kind
    }
}
