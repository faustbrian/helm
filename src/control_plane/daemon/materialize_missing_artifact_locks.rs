use super::ImageReferenceResolution;
use crate::control_plane::application::ProjectSource;
use crate::control_plane::{
    artifact_lock_required, generate_artifact_lock, parse_project_config,
    publish_missing_artifact_lock,
};

/// Creates only absent project locks through the authoritative Engine resolver.
pub(crate) fn materialize_missing_artifact_locks(
    sources: &[ProjectSource],
    resolver: &mut dyn ImageReferenceResolution,
) -> Result<usize, String> {
    let mut created = 0;
    for source in sources {
        if source.artifact_lock_path().is_some() {
            continue;
        }
        let config = parse_project_config(source.yaml(), source.config_path())
            .map_err(|error| error.to_string())?;
        if !artifact_lock_required(&config)? {
            continue;
        }
        let lock = generate_artifact_lock(&config, |references| resolver.resolve(references))?;
        let path = source.canonical_path().join(".stackctl.lock.yaml");
        if publish_missing_artifact_lock(&path, &lock).map_err(|error| error.to_string())? {
            created += 1;
        }
    }

    Ok(created)
}
