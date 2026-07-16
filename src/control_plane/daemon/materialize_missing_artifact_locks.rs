use super::ImageReferenceResolution;
use crate::control_plane::application::ProjectSource;
use crate::control_plane::{
    apply_artifact_lock, artifact_lock_required, generate_artifact_lock, parse_artifact_lock,
    parse_project_config, publish_missing_artifact_lock, replace_artifact_lock_if_unchanged,
};

/// Creates absent locks and refreshes stale generated locks through the Engine.
pub(crate) fn materialize_missing_artifact_locks(
    sources: &[ProjectSource],
    resolver: &mut dyn ImageReferenceResolution,
) -> Result<usize, String> {
    let mut created = 0;
    for source in sources {
        let config = parse_project_config(source.yaml(), source.config_path())
            .map_err(|error| error.to_string())?;
        if !artifact_lock_required(&config)? {
            continue;
        }
        let stale = match (source.artifact_lock_path(), source.artifact_lock_yaml()) {
            (Some(path), Some(yaml)) => {
                let existing =
                    parse_artifact_lock(yaml, path).map_err(|error| error.to_string())?;
                let mut resolved = config.clone();
                if apply_artifact_lock(&mut resolved, &existing, path).is_ok() {
                    continue;
                }
                Some((path, yaml))
            }
            _ => None,
        };
        let lock = generate_artifact_lock(&config, |references| resolver.resolve(references))?;
        let path = source.canonical_path().join(".stackctl.lock.yaml");
        let changed = if let Some((stale_path, expected)) = stale {
            replace_artifact_lock_if_unchanged(stale_path, expected, &lock)
                .map_err(|error| error.to_string())?
        } else {
            publish_missing_artifact_lock(&path, &lock).map_err(|error| error.to_string())?
        };
        if changed {
            created += 1;
        }
    }

    Ok(created)
}
