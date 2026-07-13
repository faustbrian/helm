/// A known Node package-manager executable available in an application image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NodePackageManager {
    Npm,
    Pnpm,
    Yarn,
}

impl NodePackageManager {
    pub(super) const fn executable(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm => "pnpm",
            Self::Yarn => "yarn",
        }
    }
}
