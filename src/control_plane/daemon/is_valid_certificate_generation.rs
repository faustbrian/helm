/// Returns whether a certificate generation is one exact content revision.
pub(crate) fn is_valid_certificate_generation(generation: &str) -> bool {
    generation.len() == 64
        && generation
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
