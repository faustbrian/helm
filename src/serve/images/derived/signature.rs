use std::hash::{Hash, Hasher};

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
