/// One Linux capability explicitly retained after dropping the default set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LinuxCapability {
    NetBindService,
}

impl LinuxCapability {
    pub(crate) const fn engine_name(self) -> &'static str {
        match self {
            Self::NetBindService => "NET_BIND_SERVICE",
        }
    }
}
