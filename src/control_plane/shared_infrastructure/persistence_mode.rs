use serde::Serialize;

/// The immutable storage behavior of one compatible service instance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub(crate) enum PersistenceMode {
    Persistent,
    Ephemeral,
}
