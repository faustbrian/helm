use anyhow::Result;

use crate::config;

pub(crate) fn handle_preset_list() {
    for name in config::preset_names() {
        println!("{name}");
    }
}

pub(crate) fn handle_preset_show(name: &str, format: &str) -> Result<()> {
    let service = config::preset_preview(name)?;
    match format {
        "json" => println!("{}", serde_json::to_string_pretty(&service)?),
        "toml" => println!("{}", toml::to_string_pretty(&service)?),
        _ => anyhow::bail!("unsupported format: {format}"),
    }
    Ok(())
}
