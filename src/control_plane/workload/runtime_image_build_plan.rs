use crate::control_plane::engine::{
    EngineError, ImageBuildRequest, ManagedResourceMetadata, ManagedResourceMetadataOptions,
    ResourceKind, RetentionClass,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// A deterministic Engine build for one reusable Linux PHP extension runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RuntimeImageBuildPlan {
    request: ImageBuildRequest,
    compatibility_fingerprint: String,
}

impl RuntimeImageBuildPlan {
    pub(crate) fn for_php_extensions(
        installation_id: &str,
        schema_version: u32,
        base_image_digest: &str,
        platform: &str,
        mut php_extensions: Vec<String>,
    ) -> Result<Self, EngineError> {
        normalize_unique(&mut php_extensions)?;
        if php_extensions.is_empty() {
            return Err(invalid_request(
                "derived PHP runtime requires at least one extension",
            ));
        }

        let manifest_json = serde_json::to_string(&PhpExtensionRuntimeManifest {
            schema_version: 1,
            php_extensions: &php_extensions,
        })
        .map_err(|error| invalid_request(format!("failed to encode runtime manifest: {error}")))?;
        let compatibility_fingerprint = fingerprint(base_image_digest, platform, &manifest_json);
        let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
            installation_id: installation_id.to_owned(),
            kind: ResourceKind::Build,
            project_id: None,
            compatibility_fingerprint: compatibility_fingerprint.clone(),
            schema_version,
            desired_revision: compatibility_fingerprint.clone(),
            retention: RetentionClass::BuildCache,
        })?;
        let mut command = vec!["install-php-extensions".to_owned()];
        command.extend(php_extensions);
        let command = serde_json::to_string(&command).map_err(|error| {
            invalid_request(format!(
                "failed to encode extension installer command: {error}"
            ))
        })?;
        let request = ImageBuildRequest::new(
            BTreeMap::new(),
            "Dockerfile".to_owned(),
            format!("FROM {base_image_digest}\nRUN {command}\n"),
            platform.to_owned(),
            metadata,
        )?;

        Ok(Self {
            request,
            compatibility_fingerprint,
        })
    }

    pub(crate) const fn request(&self) -> &ImageBuildRequest {
        &self.request
    }

    pub(crate) fn compatibility_fingerprint(&self) -> &str {
        &self.compatibility_fingerprint
    }
}

#[derive(Serialize)]
struct PhpExtensionRuntimeManifest<'value> {
    schema_version: u32,
    php_extensions: &'value [String],
}

fn normalize_unique(values: &mut [String]) -> Result<(), EngineError> {
    values.sort();
    for value in values.iter() {
        if !valid_php_extension(value) {
            return Err(invalid_request(format!(
                "runtime image PHP extension '{value}' is invalid"
            )));
        }
    }
    for pair in values.windows(2) {
        if pair[0] == pair[1] {
            return Err(invalid_request(format!(
                "runtime image declares PHP extension '{}' more than once",
                pair[0]
            )));
        }
    }

    Ok(())
}

fn valid_php_extension(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

fn fingerprint(base_image: &str, platform: &str, manifest_json: &str) -> String {
    let mut hasher = Sha256::new();
    for value in [base_image, platform, manifest_json] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }

    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn invalid_request(detail: impl Into<String>) -> EngineError {
    EngineError::InvalidRequest {
        detail: detail.into(),
    }
}
