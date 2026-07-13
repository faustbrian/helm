use super::{EngineError, ManagedResourceMetadata, ResourceKind};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};

pub(super) const BUILD_INPUT_LABEL: &str = "dev.stackctl.build-input";

/// Validated offline, content-addressed input for one derived image build.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct ImageBuildRequest {
    context_tar: Vec<u8>,
    dockerfile_path: String,
    dockerfile_contents: String,
    platform: String,
    metadata: ManagedResourceMetadata,
    input_digest: String,
    output_tag: String,
}

impl ImageBuildRequest {
    pub(crate) fn new(
        mut context_files: BTreeMap<String, Vec<u8>>,
        dockerfile_path: String,
        dockerfile_contents: String,
        platform: String,
        metadata: ManagedResourceMetadata,
    ) -> Result<Self, EngineError> {
        validate_dockerfile_path(&dockerfile_path)?;
        validate_dockerfile(&dockerfile_contents)?;
        for path in context_files.keys() {
            validate_context_path(path)?;
        }
        if context_files.contains_key(&dockerfile_path) {
            return Err(invalid_build(format!(
                "image build context must not define reserved Dockerfile path '{dockerfile_path}'"
            )));
        }
        context_files.insert(
            dockerfile_path.clone(),
            dockerfile_contents.as_bytes().to_vec(),
        );
        let context_tar = build_context_tar(&context_files)?;

        if !platform.starts_with("linux/") || platform.trim_matches('/').split('/').count() < 2 {
            return Err(invalid_build(format!(
                "image build platform '{platform}' must target Linux with an architecture"
            )));
        }

        if metadata.kind() != ResourceKind::Build {
            return Err(invalid_build(
                "derived image metadata must use the build resource kind",
            ));
        }

        let input_digest = build_input_digest(
            &context_tar,
            &dockerfile_path,
            &dockerfile_contents,
            &platform,
            &metadata.labels(),
        );
        let output_tag = format!(
            "stackctl-build:{}",
            input_digest.trim_start_matches("sha256:")
        );

        Ok(Self {
            context_tar,
            dockerfile_path,
            dockerfile_contents,
            platform,
            metadata,
            input_digest,
            output_tag,
        })
    }

    pub(super) fn context_tar(&self) -> &[u8] {
        &self.context_tar
    }

    pub(super) fn dockerfile_path(&self) -> &str {
        &self.dockerfile_path
    }

    pub(crate) fn dockerfile_contents(&self) -> &str {
        &self.dockerfile_contents
    }

    pub(super) fn platform(&self) -> &str {
        &self.platform
    }

    pub(crate) fn input_digest(&self) -> &str {
        &self.input_digest
    }

    pub(crate) fn output_tag(&self) -> &str {
        &self.output_tag
    }

    pub(super) fn labels(&self) -> BTreeMap<String, String> {
        let mut labels = self.metadata.labels();
        labels.insert(BUILD_INPUT_LABEL.to_owned(), self.input_digest.clone());
        labels
    }
}

impl Debug for ImageBuildRequest {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ImageBuildRequest")
            .field("context_bytes", &self.context_tar.len())
            .field("dockerfile_path", &self.dockerfile_path)
            .field("dockerfile_bytes", &self.dockerfile_contents.len())
            .field("platform", &self.platform)
            .field("input_digest", &self.input_digest)
            .field("output_tag", &self.output_tag)
            .finish()
    }
}

fn validate_dockerfile_path(path: &str) -> Result<(), EngineError> {
    if !is_safe_archive_path(path) {
        return Err(invalid_build(format!(
            "image build Dockerfile path '{path}' must be a safe relative archive path"
        )));
    }

    Ok(())
}

fn validate_context_path(path: &str) -> Result<(), EngineError> {
    if !is_safe_archive_path(path) {
        return Err(invalid_build(format!(
            "image build context path '{path}' must be a safe relative archive path"
        )));
    }

    Ok(())
}

fn is_safe_archive_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
        && !path.contains(['\\', '\0'])
}

fn build_context_tar(files: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, EngineError> {
    let mut archive = tar::Builder::new(Vec::new());
    for (path, contents) in files {
        let mut header = tar::Header::new_gnu();
        header.set_path(path).map_err(|error| {
            invalid_build(format!(
                "failed to encode image build context path '{path}': {error}"
            ))
        })?;
        let size = u64::try_from(contents.len()).map_err(|error| {
            invalid_build(format!(
                "image build context file '{path}' is too large: {error}"
            ))
        })?;
        header.set_size(size);
        header.set_mode(0o644);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_cksum();
        archive
            .append(&header, contents.as_slice())
            .map_err(|error| {
                invalid_build(format!("failed to encode image build context: {error}"))
            })?;
    }
    archive
        .into_inner()
        .map_err(|error| invalid_build(format!("failed to finish image build context: {error}")))
}

fn validate_dockerfile(contents: &str) -> Result<(), EngineError> {
    let mut has_base = false;

    for line in contents.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let mut parts = line.split_whitespace();
        let instruction = parts.next().unwrap_or_default();

        if instruction.eq_ignore_ascii_case("FROM") {
            let image = parts
                .find(|part| !part.starts_with("--"))
                .ok_or_else(|| invalid_build("image build Dockerfile FROM is missing a base"))?;
            has_base = true;

            if image != "scratch" && !has_sha256_digest(image) {
                return Err(invalid_build(format!(
                    "image build base '{image}' must use an immutable sha256 digest"
                )));
            }
        }

        if instruction.eq_ignore_ascii_case("ADD")
            && (line.contains("http://") || line.contains("https://"))
        {
            return Err(invalid_build(
                "image build Dockerfile must not ADD remote URLs",
            ));
        }
    }

    if !has_base {
        return Err(invalid_build(
            "image build Dockerfile must contain an immutable FROM instruction",
        ));
    }

    Ok(())
}

fn has_sha256_digest(image: &str) -> bool {
    let Some((repository, digest)) = image.rsplit_once("@sha256:") else {
        return false;
    };

    !repository.is_empty()
        && digest.len() == 64
        && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn build_input_digest(
    context_tar: &[u8],
    dockerfile_path: &str,
    dockerfile_contents: &str,
    platform: &str,
    labels: &BTreeMap<String, String>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(context_tar);

    for value in [dockerfile_path, dockerfile_contents, platform] {
        hasher.update([0]);
        hasher.update(value.as_bytes());
    }

    for (key, value) in labels {
        hasher.update([0]);
        hasher.update(key.as_bytes());
        hasher.update([0]);
        hasher.update(value.as_bytes());
    }

    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn invalid_build(detail: impl Into<String>) -> EngineError {
    EngineError::InvalidRequest {
        detail: detail.into(),
    }
}
