/// Object-store implementations with an explicit v8 sharing contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ObjectStoreFlavor {
    Minio,
    RustFs,
}

impl ObjectStoreFlavor {
    pub(super) fn from_implementation(implementation: &str) -> Option<Self> {
        match implementation {
            "minio" => Some(Self::Minio),
            "rustfs" => Some(Self::RustFs),
            _ => None,
        }
    }

    pub(super) const fn implementation(self) -> &'static str {
        match self {
            Self::Minio => "minio",
            Self::RustFs => "rustfs",
        }
    }

    pub(super) const fn root_environment(self) -> (&'static str, &'static str) {
        match self {
            Self::Minio => ("MINIO_ROOT_USER", "MINIO_ROOT_PASSWORD"),
            Self::RustFs => ("RUSTFS_ACCESS_KEY", "RUSTFS_SECRET_KEY"),
        }
    }

    pub(super) const fn readiness_path(self) -> &'static str {
        match self {
            Self::Minio => "/minio/health/ready",
            Self::RustFs => "/health/ready",
        }
    }
}
