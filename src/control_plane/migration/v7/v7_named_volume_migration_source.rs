/// Exact accepted legacy container and named-volume identities for one service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct V7NamedVolumeMigrationSource {
    service_id: String,
    container_id: String,
    volume_names: Vec<String>,
}

impl V7NamedVolumeMigrationSource {
    pub(crate) fn new(
        service_id: impl Into<String>,
        container_id: impl Into<String>,
        mut volume_names: Vec<String>,
    ) -> Result<Self, String> {
        let service_id = service_id.into();
        let container_id = container_id.into();
        volume_names.sort();
        let valid = !service_id.is_empty()
            && !container_id.is_empty()
            && !service_id.contains('\0')
            && !container_id.contains('\0')
            && !volume_names.is_empty()
            && volume_names
                .iter()
                .all(|name| !name.is_empty() && !name.contains('\0'))
            && !volume_names.windows(2).any(|pair| pair[0] == pair[1]);
        if !valid {
            return Err(
                "v7 named-volume source requires exact service, container, and unique volume identities"
                    .to_owned(),
            );
        }

        Ok(Self {
            service_id,
            container_id,
            volume_names,
        })
    }

    pub(crate) fn service_id(&self) -> &str {
        &self.service_id
    }

    pub(crate) fn container_id(&self) -> &str {
        &self.container_id
    }

    pub(crate) fn volume_names(&self) -> &[String] {
        &self.volume_names
    }
}
