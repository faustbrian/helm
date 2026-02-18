//! Shared docker image inspection helpers.

use anyhow::Result;

use super::run_docker_output;

/// Returns true when `docker image inspect <image>` succeeds.
pub(crate) fn docker_image_exists(image: &str, context: &str) -> Result<bool> {
    let inspect = run_docker_output(&["image", "inspect", image], context)?;
    Ok(inspect.status.success())
}

/// Returns repo digest from `docker image inspect --format` when available.
pub(crate) fn docker_image_repo_digest(image: &str, context: &str) -> Result<Option<String>> {
    let output = run_docker_output(
        &[
            "image",
            "inspect",
            "--format",
            "{{index .RepoDigests 0}}",
            image,
        ],
        context,
    )?;
    if !output.status.success() {
        return Ok(None);
    }

    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if value.is_empty() || value == "<no value>" || value == "<nil>" {
        return Ok(None);
    }
    Ok(Some(value))
}
