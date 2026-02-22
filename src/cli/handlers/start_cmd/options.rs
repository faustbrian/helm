//! start command option helpers.

pub(super) fn resolve_start_wait_flags(wait: bool, no_wait: bool) -> (bool, bool) {
    (wait, no_wait || !wait)
}
