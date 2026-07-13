/// Returns whether an image is pinned by registry manifest or local Engine ID.
pub(crate) fn is_immutable_image_identity(image: &str) -> bool {
    image.strip_prefix("sha256:").is_some_and(is_sha256_digest)
        || image
            .rsplit_once("@sha256:")
            .is_some_and(|(repository, digest)| !repository.is_empty() && is_sha256_digest(digest))
}

fn is_sha256_digest(digest: &str) -> bool {
    digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}
