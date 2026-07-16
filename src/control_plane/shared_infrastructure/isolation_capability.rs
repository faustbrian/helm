use serde::Serialize;

/// The logical tenant boundary proven by a shared implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub(crate) enum IsolationCapability {
    DatabaseAndRole,
    AclAndPrefix,
    BucketAndPolicy,
    VirtualHostAndUser,
    None,
}
