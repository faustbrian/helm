//! Signature and tag derivation for serve derived images.

use std::hash::{Hash, Hasher};

/// Derives a docker-safe derived image tag from container name and signature.
pub(super) fn derived_image_tag(container_name: &str, signature: &str) -> String {
    let sanitized: String = container_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
                ch
            } else {
                '-'
            }
        })
        .collect();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    signature.hash(&mut hasher);
    let digest = hasher.finish();
    format!("helm/{}-serve-{:x}", sanitized.to_lowercase(), digest)
}

/// Derives the cache signature describing derived image contents.
pub(super) fn derive_image_signature(
    base_image: &str,
    include_js_tooling: bool,
    sql_client_flavor: &str,
    extensions: &[String],
) -> String {
    format!(
        "v=6;base={base_image};js={include_js_tooling};sql={sql_client_flavor};exts={}",
        extensions.join(",")
    )
}
