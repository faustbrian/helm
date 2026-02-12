use anyhow::{Result, anyhow};

use super::super::super::{Driver, RawServiceConfig, presets};
use super::fields::{pick_required, pick_with_default};

pub(super) fn resolve_name_and_image(
    raw: &RawServiceConfig,
    defaults: Option<&presets::PresetDefaults>,
    driver: Driver,
) -> Result<(String, String)> {
    let name = pick_required(
        raw.name.clone(),
        defaults.and_then(|d| d.name),
        format!("{driver:?}").to_lowercase(),
    );
    let image = pick_with_default(raw.image.clone(), defaults.map(|d| d.image), || {
        anyhow!("service '{name}' is missing image")
    })?;

    Ok((name, image))
}
