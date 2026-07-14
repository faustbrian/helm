use super::{EngineError, V7ContainerCommandTarget};

/// Exact accepted legacy container and named volumes authorized for retirement.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct V7ContainerRetirementTarget {
    container: V7ContainerCommandTarget,
    named_volumes: Vec<String>,
}

impl V7ContainerRetirementTarget {
    pub(crate) fn new(
        container: V7ContainerCommandTarget,
        mut named_volumes: Vec<String>,
    ) -> Result<Self, EngineError> {
        named_volumes.sort();
        let valid = named_volumes
            .iter()
            .all(|name| !name.is_empty() && !name.contains('\0'))
            && !named_volumes.windows(2).any(|pair| pair[0] == pair[1]);
        if !valid {
            return Err(EngineError::InvalidRequest {
                detail: "v7 retirement requires unique non-empty named-volume identities"
                    .to_owned(),
            });
        }

        Ok(Self {
            container,
            named_volumes,
        })
    }

    pub(crate) const fn container(&self) -> &V7ContainerCommandTarget {
        &self.container
    }

    pub(crate) fn named_volumes(&self) -> &[String] {
        &self.named_volumes
    }
}
