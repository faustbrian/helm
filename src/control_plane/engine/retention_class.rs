/// The deletion and garbage-collection policy attached to a managed resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum RetentionClass {
    Persistent,
    Disposable,
    BuildCache,
}

impl RetentionClass {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Persistent => "persistent",
            Self::Disposable => "disposable",
            Self::BuildCache => "build_cache",
        }
    }
}
