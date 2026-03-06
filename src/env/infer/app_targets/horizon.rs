//! Horizon app target env inference.

use std::collections::HashMap;

use super::super::insert_if_absent;

/// Applies inferred Horizon env keys.
pub(super) fn apply(vars: &mut HashMap<String, String>) {
    insert_if_absent(vars, "QUEUE_CONNECTION", "redis".to_owned());
}
