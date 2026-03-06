//! Docker image digest resolution helpers for lockfile generation.

use anyhow::Result;

use crate::docker::{docker_image_repo_digest, docker_pull};

/// Resolves image digest using configured inputs and runtime state.
pub(super) fn resolve_image_digest(image: &str) -> Result<String> {
    if image.contains("@sha256:") {
        return Ok(image.to_owned());
    }

    if let Some(digest) = inspect_repo_digest(image)? {
        return Ok(digest);
    }

    docker_pull(
        image,
        &format!("failed to pull image '{image}' while resolving lockfile"),
        &format!("failed to pull image '{image}' while resolving lockfile"),
    )?;

    inspect_repo_digest(image)?
        .ok_or_else(|| anyhow::anyhow!("image '{image}' has no repo digest after pull"))
}

fn inspect_repo_digest(image: &str) -> Result<Option<String>> {
    docker_image_repo_digest(image, &format!("failed to inspect image '{image}'"))
}
