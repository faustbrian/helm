use anyhow::{Context, Result};
use std::path::Path;

pub(crate) fn scrub_env_file(env_path: &Path) -> Result<usize> {
    let content = std::fs::read_to_string(env_path)
        .with_context(|| format!("failed to read {}", env_path.display()))?;
    let mut updated = 0_usize;
    let scrubbed = content
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return line.to_owned();
            }
            let Some((key, _)) = line.split_once('=') else {
                return line.to_owned();
            };
            let replacement = match key.trim() {
                "APP_KEY" => Some("APP_KEY=\"\""),
                "DB_PASSWORD" => Some("DB_PASSWORD=\"local\""),
                "MAIL_PASSWORD" => Some("MAIL_PASSWORD=\"\""),
                "AWS_SECRET_ACCESS_KEY" => Some("AWS_SECRET_ACCESS_KEY=\"miniosecret\""),
                "MEILISEARCH_KEY" => Some("MEILISEARCH_KEY=\"masterKey\""),
                "TYPESENSE_API_KEY" => Some("TYPESENSE_API_KEY=\"xyz\""),
                "REDIS_PASSWORD" => Some("REDIS_PASSWORD=\"\""),
                _ => None,
            };
            if let Some(new_line) = replacement {
                updated += 1;
                return new_line.to_owned();
            }
            line.to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n");

    std::fs::write(env_path, format!("{scrubbed}\n"))
        .with_context(|| format!("failed to write {}", env_path.display()))?;
    Ok(updated)
}
