use super::ContainerLogTail;

/// Typed options for one bounded-history or following container log stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ContainerLogOptions {
    follow: bool,
    tail: ContainerLogTail,
}

impl ContainerLogOptions {
    pub(crate) const fn new(follow: bool, tail: ContainerLogTail) -> Self {
        Self { follow, tail }
    }

    pub(super) const fn follow(&self) -> bool {
        self.follow
    }

    pub(super) const fn tail(&self) -> ContainerLogTail {
        self.tail
    }
}
