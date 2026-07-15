/// Supported Redis-protocol implementations with distinct compatibility state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RedisFlavor {
    Redis,
    Valkey,
}

impl RedisFlavor {
    pub(super) fn from_implementation(implementation: &str) -> Option<Self> {
        match implementation {
            "redis" => Some(Self::Redis),
            "valkey" => Some(Self::Valkey),
            _ => None,
        }
    }

    pub(crate) const fn implementation(self) -> &'static str {
        match self {
            Self::Redis => "redis",
            Self::Valkey => "valkey",
        }
    }

    pub(super) const fn server_executable(self) -> &'static str {
        match self {
            Self::Redis => "redis-server",
            Self::Valkey => "valkey-server",
        }
    }

    pub(crate) const fn client_executable(self) -> &'static str {
        match self {
            Self::Redis => "redis-cli",
            Self::Valkey => "valkey-cli",
        }
    }

    pub(crate) const fn client_auth_environment_key(self) -> &'static str {
        match self {
            Self::Redis | Self::Valkey => "REDISCLI_AUTH",
        }
    }
}
