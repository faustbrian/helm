use crate::control_plane::engine::ContainerCreateOptions;
use std::collections::BTreeMap;

/// One private disposable browser and its application command environment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct EphemeralBrowserPlan {
    request: ContainerCreateOptions,
    command_environment: BTreeMap<String, String>,
}

impl EphemeralBrowserPlan {
    pub(super) const fn new(
        request: ContainerCreateOptions,
        command_environment: BTreeMap<String, String>,
    ) -> Self {
        Self {
            request,
            command_environment,
        }
    }

    pub(crate) const fn request(&self) -> &ContainerCreateOptions {
        &self.request
    }

    pub(crate) const fn command_environment(&self) -> &BTreeMap<String, String> {
        &self.command_environment
    }
}
