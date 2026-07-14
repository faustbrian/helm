use super::V7LogicalDataMigrationSourceOptions;
use crate::control_plane::engine::{ContainerId, EngineError, V7ContainerCommandTarget};
use std::collections::BTreeMap;

/// Exact accepted legacy logical-data source for one service strategy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct V7LogicalDataMigrationSource {
    project_id: String,
    service_id: String,
    kind: String,
    driver: String,
    container_name: String,
    container_id: String,
    named_volumes: Vec<String>,
    logical_data: BTreeMap<String, String>,
}

impl V7LogicalDataMigrationSource {
    pub(crate) fn new(options: V7LogicalDataMigrationSourceOptions) -> Result<Self, String> {
        let V7LogicalDataMigrationSourceOptions {
            project_id,
            service_id,
            kind,
            driver,
            container_name,
            container_id,
            mut named_volumes,
            logical_data,
        } = options;
        named_volumes.sort();
        let valid = !project_id.is_empty()
            && !project_id.contains('/')
            && !project_id.contains('\0')
            && !service_id.is_empty()
            && !service_id.contains('/')
            && !service_id.contains('\0')
            && !kind.is_empty()
            && !kind.contains('\0')
            && !driver.is_empty()
            && !driver.contains('\0')
            && !container_name.is_empty()
            && !container_name.contains('\0')
            && !container_id.is_empty()
            && !container_id.contains('\0')
            && named_volumes
                .iter()
                .all(|name| !name.is_empty() && !name.contains('\0'))
            && !named_volumes.windows(2).any(|pair| pair[0] == pair[1])
            && logical_data.iter().all(|(key, value)| {
                !key.is_empty() && !value.is_empty() && !key.contains('\0') && !value.contains('\0')
            });
        if !valid {
            return Err(
                "v7 logical-data source requires exact project, service, kind, driver, container, and logical identities"
                    .to_owned(),
            );
        }

        Ok(Self {
            project_id,
            service_id,
            kind,
            driver,
            container_name,
            container_id,
            named_volumes,
            logical_data,
        })
    }

    pub(crate) fn project_id(&self) -> &str {
        &self.project_id
    }

    pub(crate) fn service_id(&self) -> &str {
        &self.service_id
    }

    pub(crate) fn driver(&self) -> &str {
        &self.driver
    }

    pub(crate) fn kind(&self) -> &str {
        &self.kind
    }

    pub(crate) fn container_name(&self) -> &str {
        &self.container_name
    }

    pub(crate) fn container_id(&self) -> &str {
        &self.container_id
    }

    pub(crate) fn named_volumes(&self) -> &[String] {
        &self.named_volumes
    }

    pub(crate) const fn logical_data(&self) -> &BTreeMap<String, String> {
        &self.logical_data
    }

    pub(crate) fn command_target(&self) -> Result<V7ContainerCommandTarget, EngineError> {
        V7ContainerCommandTarget::new(
            ContainerId::new(self.container_id()),
            self.container_name(),
            self.service_id(),
            self.kind(),
        )
    }
}
