use super::{V7InventoryBlocker, V7VolumeSource};
use crate::control_plane::engine::ObservedContainerMount;

/// Secret-free v7 mount identity retained for migration selection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct V7VolumeInventory {
    source: V7VolumeSource,
    target: String,
    read_only: bool,
}

impl V7VolumeInventory {
    pub(super) fn from_mount(service_id: &str, mount: &str) -> (Self, Option<V7InventoryBlocker>) {
        let parts = mount.split(':').collect::<Vec<_>>();
        let (source, target, read_only) = match parts.as_slice() {
            [target] if valid_target(target) => (V7VolumeSource::Anonymous, *target, false),
            [source, target] if valid_target(target) => (classify(source), *target, false),
            [source, target, mode] if valid_target(target) && valid_mode(mode) => (
                classify(source),
                *target,
                mode.split(',').any(|item| item == "ro"),
            ),
            _ => (V7VolumeSource::Unsupported, mount, false),
        };
        let target = target.to_owned();
        let blocker = match &source {
            V7VolumeSource::HostBind(source) => Some(V7InventoryBlocker::HostBindVolume {
                service_id: service_id.to_owned(),
                source: source.clone(),
                target: target.clone(),
            }),
            V7VolumeSource::Anonymous => Some(V7InventoryBlocker::AnonymousVolume {
                service_id: service_id.to_owned(),
                target: target.clone(),
            }),
            V7VolumeSource::Unsupported => Some(V7InventoryBlocker::UnsupportedVolume {
                service_id: service_id.to_owned(),
                mount: mount.to_owned(),
            }),
            V7VolumeSource::Named(_) => None,
        };

        (
            Self {
                source,
                target,
                read_only,
            },
            blocker,
        )
    }

    pub(super) fn named(source: String, target: &str) -> Self {
        Self {
            source: V7VolumeSource::Named(source),
            target: target.to_owned(),
            read_only: false,
        }
    }

    pub(crate) const fn source(&self) -> &V7VolumeSource {
        &self.source
    }

    pub(crate) fn target(&self) -> &str {
        &self.target
    }

    pub(crate) const fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub(super) fn expected_source(&self) -> &str {
        match &self.source {
            V7VolumeSource::Named(source) | V7VolumeSource::HostBind(source) => source,
            V7VolumeSource::Anonymous => "<anonymous>",
            V7VolumeSource::Unsupported => "<unsupported>",
        }
    }

    pub(super) fn matches_observed(&self, observed: &ObservedContainerMount) -> bool {
        if self.target != observed.target() || self.read_only != observed.is_read_only() {
            return false;
        }
        match &self.source {
            V7VolumeSource::Named(source) => {
                observed.is_named_volume() && source == observed.source()
            }
            V7VolumeSource::HostBind(source) => {
                !observed.is_named_volume() && source == observed.source()
            }
            V7VolumeSource::Anonymous => true,
            V7VolumeSource::Unsupported => false,
        }
    }
}

fn classify(source: &str) -> V7VolumeSource {
    if source.is_empty() {
        V7VolumeSource::Anonymous
    } else if valid_named_volume(source) {
        V7VolumeSource::Named(source.to_owned())
    } else {
        V7VolumeSource::HostBind(source.to_owned())
    }
}

fn valid_named_volume(value: &str) -> bool {
    value.bytes().enumerate().all(|(index, byte)| match byte {
        b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' => true,
        b'_' | b'.' | b'-' => index > 0,
        _ => false,
    })
}

fn valid_target(value: &str) -> bool {
    value.starts_with('/') && !value.contains("..")
}

fn valid_mode(value: &str) -> bool {
    !value.is_empty()
        && value
            .split(',')
            .all(|item| matches!(item, "ro" | "rw" | "z" | "Z" | "nocopy"))
}
