use super::compatibility_fingerprint_error::CompatibilityFingerprintError;
use super::{
    CompatibilityFingerprint, CompatibilityFingerprintOptions, IsolationCapability, PersistenceMode,
};
use std::collections::BTreeMap;

/// Canonical immutable service profile retained alongside its content identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CompatibilityProfile {
    options: CompatibilityFingerprintOptions,
    fingerprint: CompatibilityFingerprint,
}

impl CompatibilityProfile {
    pub(crate) fn from_options(
        mut options: CompatibilityFingerprintOptions,
    ) -> Result<Self, CompatibilityFingerprintError> {
        options.extensions.sort();
        options.extensions.dedup();
        let fingerprint = CompatibilityFingerprint::from_options(options.clone())?;

        Ok(Self {
            options,
            fingerprint,
        })
    }

    pub(crate) const fn fingerprint(&self) -> &CompatibilityFingerprint {
        &self.fingerprint
    }

    pub(crate) fn implementation(&self) -> &str {
        &self.options.implementation
    }

    pub(crate) fn major_version(&self) -> &str {
        &self.options.major_version
    }

    pub(crate) fn image_digest(&self) -> &str {
        &self.options.image_digest
    }

    pub(crate) fn extensions(&self) -> &[String] {
        &self.options.extensions
    }

    pub(crate) const fn immutable_settings(&self) -> &BTreeMap<String, String> {
        &self.options.immutable_settings
    }

    pub(crate) const fn persistence(&self) -> PersistenceMode {
        self.options.persistence
    }

    pub(crate) const fn isolation(&self) -> IsolationCapability {
        self.options.isolation
    }

    pub(crate) fn platform_architecture(&self) -> Option<&str> {
        self.options.platform_architecture.as_deref()
    }
}
