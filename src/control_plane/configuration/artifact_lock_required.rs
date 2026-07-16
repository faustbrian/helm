use super::{RawProjectConfig, artifact_source};
use crate::control_plane::resolve_preset_artifact;

/// Reports whether a valid project declares any Engine image to lock.
pub(crate) fn artifact_lock_required(config: &RawProjectConfig) -> Result<bool, String> {
    for (service_id, service) in config.services() {
        if service.image().is_some() {
            return Ok(true);
        }
        let Some(preset) = service.preset() else {
            if artifact_source(service).is_none() {
                return Err(format!(
                    "service '{service_id}' has neither an image nor a preset"
                ));
            }
            continue;
        };
        if resolve_preset_artifact(preset, service.version())
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Ok(true);
        }
    }

    Ok(false)
}
