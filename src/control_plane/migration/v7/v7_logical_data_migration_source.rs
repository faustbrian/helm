use std::collections::BTreeMap;

/// Exact accepted legacy logical-data source for one service strategy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct V7LogicalDataMigrationSource {
    service_id: String,
    driver: String,
    container_id: String,
    logical_data: BTreeMap<String, String>,
}

impl V7LogicalDataMigrationSource {
    pub(crate) fn new(
        service_id: impl Into<String>,
        driver: impl Into<String>,
        container_id: impl Into<String>,
        logical_data: BTreeMap<String, String>,
    ) -> Result<Self, String> {
        let service_id = service_id.into();
        let driver = driver.into();
        let container_id = container_id.into();
        let valid = !service_id.is_empty()
            && !service_id.contains('/')
            && !service_id.contains('\0')
            && !driver.is_empty()
            && !driver.contains('\0')
            && !container_id.is_empty()
            && !container_id.contains('\0')
            && logical_data.iter().all(|(key, value)| {
                !key.is_empty() && !value.is_empty() && !key.contains('\0') && !value.contains('\0')
            });
        if !valid {
            return Err(
                "v7 logical-data source requires exact service, driver, container, and logical identities"
                    .to_owned(),
            );
        }

        Ok(Self {
            service_id,
            driver,
            container_id,
            logical_data,
        })
    }

    pub(crate) fn service_id(&self) -> &str {
        &self.service_id
    }

    pub(crate) fn driver(&self) -> &str {
        &self.driver
    }

    pub(crate) fn container_id(&self) -> &str {
        &self.container_id
    }

    pub(crate) const fn logical_data(&self) -> &BTreeMap<String, String> {
        &self.logical_data
    }
}
