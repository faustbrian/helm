use super::RawServiceConfig;

/// Returns the exact freshness identity represented by an artifact-lock entry.
pub(crate) fn artifact_source(service: &RawServiceConfig) -> Option<String> {
    if let Some(image) = service.image() {
        return Some(image.to_owned());
    }

    service.preset().map(|preset| match service.version() {
        Some(version) => format!("preset:{preset}:{version}"),
        None => format!("preset:{preset}"),
    })
}
