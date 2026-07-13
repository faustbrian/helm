use super::{JavaScriptRuntimeSpec, RuntimeImageBuildPlanOptions};
use crate::control_plane::engine::{
    EngineError, ImageBuildRequest, ManagedResourceMetadata, ManagedResourceMetadataOptions,
    ResourceKind, RetentionClass,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const MANIFEST_PATH: &str = "runtime-manifest.json";
const INSTALLER_PATH: &str = "/usr/local/bin/stackctl-runtime-install";

/// A deterministic, offline Engine build for one reusable Linux runtime.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct RuntimeImageBuildPlan {
    request: ImageBuildRequest,
    compatibility_fingerprint: String,
    manifest_json: String,
}

impl RuntimeImageBuildPlan {
    pub(crate) fn new(mut options: RuntimeImageBuildPlanOptions) -> Result<Self, EngineError> {
        validate_exact_version("PHP", &options.php_version)?;
        validate_exact_version("Composer", &options.composer_version)?;
        if let Some(javascript) = &options.javascript {
            validate_exact_version("JavaScript runtime", javascript.version())?;
        }
        validate_installer_revision(&options.installer_revision)?;
        normalize_unique(
            &mut options.php_extensions,
            "PHP extension",
            valid_php_extension,
        )?;
        normalize_unique(
            &mut options.system_packages,
            "system package",
            valid_system_package,
        )?;

        let manifest_json = serde_json::to_string(&RuntimeImageManifest {
            schema_version: 1,
            php_version: &options.php_version,
            php_extensions: &options.php_extensions,
            system_packages: &options.system_packages,
            composer_version: &options.composer_version,
            javascript: options.javascript.as_ref(),
            installer_revision: &options.installer_revision,
        })
        .map_err(|error| invalid_request(format!("failed to encode runtime manifest: {error}")))?;
        let compatibility_fingerprint = fingerprint(
            &options.base_image_digest,
            &options.platform,
            &manifest_json,
        );
        let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
            installation_id: options.installation_id,
            kind: ResourceKind::Build,
            project_id: None,
            compatibility_fingerprint: compatibility_fingerprint.clone(),
            schema_version: options.schema_version,
            desired_revision: compatibility_fingerprint.clone(),
            retention: RetentionClass::Disposable,
        })?;
        let dockerfile = format!(
            "FROM {}\nCOPY {} /opt/stackctl/{}\nRUN [\"{}\",\"--offline\",\"--expected-revision\",\"{}\",\"/opt/stackctl/{}\"]\n",
            options.base_image_digest,
            MANIFEST_PATH,
            MANIFEST_PATH,
            INSTALLER_PATH,
            options.installer_revision,
            MANIFEST_PATH,
        );
        let request = ImageBuildRequest::new(
            BTreeMap::from([(MANIFEST_PATH.to_owned(), manifest_json.as_bytes().to_vec())]),
            "Dockerfile".to_owned(),
            dockerfile,
            options.platform,
            metadata,
        )?;

        Ok(Self {
            request,
            compatibility_fingerprint,
            manifest_json,
        })
    }

    pub(crate) const fn request(&self) -> &ImageBuildRequest {
        &self.request
    }

    pub(crate) fn into_request(self) -> ImageBuildRequest {
        self.request
    }

    pub(crate) fn compatibility_fingerprint(&self) -> &str {
        &self.compatibility_fingerprint
    }

    pub(crate) fn manifest_json(&self) -> &str {
        &self.manifest_json
    }
}

#[derive(Serialize)]
struct RuntimeImageManifest<'value> {
    schema_version: u32,
    php_version: &'value str,
    php_extensions: &'value [String],
    system_packages: &'value [String],
    composer_version: &'value str,
    javascript: Option<&'value JavaScriptRuntimeSpec>,
    installer_revision: &'value str,
}

fn validate_exact_version(name: &str, version: &str) -> Result<(), EngineError> {
    let components = version.split('.').collect::<Vec<_>>();
    if components.len() < 2
        || components.iter().any(|component| {
            component.is_empty() || !component.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return Err(invalid_request(format!(
            "{name} version '{version}' must be an exact numeric version"
        )));
    }

    Ok(())
}

fn validate_installer_revision(revision: &str) -> Result<(), EngineError> {
    if revision.is_empty()
        || !revision
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(invalid_request(format!(
            "runtime installer revision '{revision}' is invalid"
        )));
    }

    Ok(())
}

fn normalize_unique(
    values: &mut [String],
    label: &str,
    valid: fn(&str) -> bool,
) -> Result<(), EngineError> {
    values.sort();
    for value in values.iter() {
        if !valid(value) {
            return Err(invalid_request(format!(
                "runtime image {label} '{value}' is invalid"
            )));
        }
    }
    for pair in values.windows(2) {
        if pair[0] == pair[1] {
            return Err(invalid_request(format!(
                "runtime image declares {label} '{}' more than once",
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

fn valid_system_package(value: &str) -> bool {
    value.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'+' | b'.' | b'-')
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
