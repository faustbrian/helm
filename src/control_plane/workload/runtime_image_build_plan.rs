use super::RuntimeImageBuildOptions;
use crate::control_plane::engine::{
    EngineError, ImageBuildRequest, ImmutableImageReference, ManagedResourceMetadata,
    ManagedResourceMetadataOptions, ResourceKind, RetentionClass,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// A deterministic Engine build for one reusable Linux application runtime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RuntimeImageBuildPlan {
    request: ImageBuildRequest,
    compatibility_fingerprint: String,
    input_images: Vec<ImmutableImageReference>,
}

impl RuntimeImageBuildPlan {
    pub(crate) fn for_application_runtime(
        options: RuntimeImageBuildOptions<'_>,
    ) -> Result<Self, EngineError> {
        let mut php_extensions = options.php_extensions;
        normalize_unique(&mut php_extensions)?;
        if php_extensions.is_empty()
            && options.composer_image.is_none()
            && options.node_image.is_none()
            && options.bun_image.is_none()
        {
            return Err(invalid_request(
                "derived application runtime requires at least one declared tool or PHP extension",
            ));
        }

        let manifest_json = serde_json::to_string(&ApplicationRuntimeManifest {
            schema_version: 1,
            php_extensions: &php_extensions,
            composer_image: options.composer_image,
            node_image: options.node_image,
            bun_image: options.bun_image,
        })
        .map_err(|error| invalid_request(format!("failed to encode runtime manifest: {error}")))?;
        let compatibility_fingerprint =
            fingerprint(options.base_image_digest, options.platform, &manifest_json);
        let metadata = ManagedResourceMetadata::new(ManagedResourceMetadataOptions {
            installation_id: options.installation_id.to_owned(),
            kind: ResourceKind::Build,
            project_id: None,
            compatibility_fingerprint: compatibility_fingerprint.clone(),
            schema_version: options.schema_version,
            desired_revision: compatibility_fingerprint.clone(),
            retention: RetentionClass::BuildCache,
        })?;
        let input_images = [
            Some(options.base_image_digest),
            options.composer_image,
            options.node_image,
            options.bun_image,
        ]
        .into_iter()
        .flatten()
        .map(ImmutableImageReference::new)
        .collect::<Result<Vec<_>, _>>()?;
        let mut dockerfile = String::new();
        if let Some(image) = options.composer_image {
            dockerfile.push_str(&format!("FROM {image} AS stackctl_composer\n"));
        }
        if let Some(image) = options.node_image {
            dockerfile.push_str(&format!("FROM {image} AS stackctl_node\n"));
        }
        if let Some(image) = options.bun_image {
            dockerfile.push_str(&format!("FROM {image} AS stackctl_bun\n"));
        }
        dockerfile.push_str(&format!("FROM {}\n", options.base_image_digest));
        if options.composer_image.is_some() {
            dockerfile.push_str(
                "COPY --from=stackctl_composer /usr/bin/composer /usr/local/bin/composer\n",
            );
        }
        if options.node_image.is_some() {
            dockerfile.push_str("COPY --from=stackctl_node /usr/local/ /usr/local/\n");
        }
        if options.bun_image.is_some() {
            dockerfile.push_str("COPY --from=stackctl_bun /usr/local/bin/bun /usr/local/bin/bun\n");
        }
        if !php_extensions.is_empty() {
            let mut enable_command = vec!["docker-php-ext-enable".to_owned()];
            enable_command.extend(php_extensions.clone());
            let enable_command = serde_json::to_string(&enable_command).map_err(|error| {
                invalid_request(format!(
                    "failed to encode extension enablement command: {error}"
                ))
            })?;
            dockerfile.push_str(&format!("RUN {enable_command}\n"));
            let mut verify_command = vec![
                "php".to_owned(),
                "-r".to_owned(),
                "foreach (array_slice($argv, 1) as $extension) { if (!extension_loaded($extension)) { fwrite(STDERR, 'missing PHP extension: ' . $extension . PHP_EOL); exit(1); } }".to_owned(),
            ];
            verify_command.extend(php_extensions);
            let verify_command = serde_json::to_string(&verify_command).map_err(|error| {
                invalid_request(format!(
                    "failed to encode extension verification command: {error}"
                ))
            })?;
            dockerfile.push_str(&format!("RUN {verify_command}\n"));
        }
        let request = ImageBuildRequest::new(
            BTreeMap::new(),
            "Dockerfile".to_owned(),
            dockerfile,
            options.platform.to_owned(),
            metadata,
        )?;

        Ok(Self {
            request,
            compatibility_fingerprint,
            input_images,
        })
    }

    pub(crate) const fn request(&self) -> &ImageBuildRequest {
        &self.request
    }

    pub(crate) fn compatibility_fingerprint(&self) -> &str {
        &self.compatibility_fingerprint
    }

    pub(crate) fn input_images(&self) -> &[ImmutableImageReference] {
        &self.input_images
    }
}

#[derive(Serialize)]
struct ApplicationRuntimeManifest<'value> {
    schema_version: u32,
    php_extensions: &'value [String],
    composer_image: Option<&'value str>,
    node_image: Option<&'value str>,
    bun_image: Option<&'value str>,
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
