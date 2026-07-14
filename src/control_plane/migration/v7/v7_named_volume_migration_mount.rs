/// Exact accepted legacy named-volume identity and container mount target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct V7NamedVolumeMigrationMount {
    volume_name: String,
    target: String,
}

impl V7NamedVolumeMigrationMount {
    pub(crate) fn new(
        volume_name: impl Into<String>,
        target: impl Into<String>,
    ) -> Result<Self, String> {
        let volume_name = volume_name.into();
        let target = target.into();
        if volume_name.is_empty()
            || target.is_empty()
            || volume_name.contains('\0')
            || target.contains('\0')
            || !target.starts_with('/')
        {
            return Err(
                "v7 named-volume mount requires an exact volume name and absolute target"
                    .to_owned(),
            );
        }

        Ok(Self {
            volume_name,
            target,
        })
    }

    pub(crate) fn volume_name(&self) -> &str {
        &self.volume_name
    }

    pub(crate) fn target(&self) -> &str {
        &self.target
    }
}
