use super::{DesiredProjectError, resolve_desired_project::invalid_service};

/// Validates one optional application tool image at the pure desired-state boundary.
pub(super) fn resolve_runtime_image_reference(
    service: &str,
    field: &str,
    reference: Option<&str>,
) -> Result<Option<String>, DesiredProjectError> {
    let Some(reference) = reference else {
        return Ok(None);
    };
    let Some((repository, digest)) = reference.rsplit_once("@sha256:") else {
        return Err(invalid_service(
            service,
            format!("{field} must use an immutable sha256 digest"),
        ));
    };
    if !valid_repository(repository)
        || digest.len() != 64
        || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(invalid_service(
            service,
            format!("{field} must use an immutable sha256 digest"),
        ));
    }

    Ok(Some(reference.to_owned()))
}

fn valid_repository(repository: &str) -> bool {
    repository
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric())
        && repository.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'/' | b'-')
        })
}
