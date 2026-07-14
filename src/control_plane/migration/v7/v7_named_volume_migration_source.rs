use super::V7NamedVolumeMigrationMount;

/// Exact accepted legacy container and named-volume identities for one service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct V7NamedVolumeMigrationSource {
    service_id: String,
    container_id: String,
    mounts: Vec<V7NamedVolumeMigrationMount>,
    volume_names: Vec<String>,
}

impl V7NamedVolumeMigrationSource {
    pub(crate) fn new(
        service_id: impl Into<String>,
        container_id: impl Into<String>,
        mut mounts: Vec<V7NamedVolumeMigrationMount>,
    ) -> Result<Self, String> {
        let service_id = service_id.into();
        let container_id = container_id.into();
        mounts.sort_by(|left, right| left.volume_name().cmp(right.volume_name()));
        let volume_names = mounts
            .iter()
            .map(|mount| mount.volume_name().to_owned())
            .collect::<Vec<_>>();
        let valid = !service_id.is_empty()
            && !container_id.is_empty()
            && !service_id.contains('\0')
            && !container_id.contains('\0')
            && !mounts.is_empty()
            && !volume_names.windows(2).any(|pair| pair[0] == pair[1])
            && !mounts.iter().enumerate().any(|(index, mount)| {
                mounts[index + 1..]
                    .iter()
                    .any(|other| mount.target() == other.target())
            });
        if !valid {
            return Err(
                "v7 named-volume source requires exact service, container, volume, and mount-target identities"
                    .to_owned(),
            );
        }

        Ok(Self {
            service_id,
            container_id,
            mounts,
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

    pub(crate) fn mounts(&self) -> &[V7NamedVolumeMigrationMount] {
        &self.mounts
    }
}
