use super::CompatibilityFingerprintOptions;
use super::compatibility_fingerprint_error::CompatibilityFingerprintError;
use sha2::{Digest, Sha256};

/// A deterministic content identity for one safely shareable instance.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct CompatibilityFingerprint(String);

impl CompatibilityFingerprint {
    pub(crate) fn from_options(
        mut options: CompatibilityFingerprintOptions,
    ) -> Result<Self, CompatibilityFingerprintError> {
        validate_required("implementation", &options.implementation)?;
        validate_required("major version", &options.major_version)?;
        validate_image_digest(&options.image_digest)?;

        options.extensions.sort();
        options.extensions.dedup();

        let canonical = serde_json::to_vec(&options).map_err(|error| {
            CompatibilityFingerprintError::new(format!(
                "failed to encode compatibility profile: {error}"
            ))
        })?;
        let digest = Sha256::digest(canonical);

        Ok(Self(format!("sha256:{}", hex::encode(digest))))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

fn validate_required(field: &str, value: &str) -> Result<(), CompatibilityFingerprintError> {
    if value.is_empty() {
        return Err(CompatibilityFingerprintError::new(format!(
            "compatibility {field} must not be empty"
        )));
    }

    Ok(())
}

fn validate_image_digest(image: &str) -> Result<(), CompatibilityFingerprintError> {
    let valid = image.rsplit_once("@sha256:").is_some_and(|(_, digest)| {
        digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    });

    if !valid {
        return Err(CompatibilityFingerprintError::new(format!(
            "compatibility image '{image}' must use an immutable sha256 digest"
        )));
    }

    Ok(())
}
