use sha2::{Digest, Sha256};
use std::io::{Read, Result};

/// Tracks the exact bytes consumed by a restore target.
pub(super) struct HashingReader<R> {
    inner: R,
    digest: Sha256,
    size: u64,
    reached_eof: bool,
}

impl<R> HashingReader<R> {
    pub(super) fn new(inner: R) -> Self {
        Self {
            inner,
            digest: Sha256::new(),
            size: 0,
            reached_eof: false,
        }
    }

    pub(super) fn finish(self) -> (String, u64, bool) {
        (
            hex::encode(self.digest.finalize()),
            self.size,
            self.reached_eof,
        )
    }
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize> {
        let count = self.inner.read(buffer)?;
        if count == 0 {
            self.reached_eof = true;
        } else {
            self.digest.update(&buffer[..count]);
            self.size = self
                .size
                .saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
        }

        Ok(count)
    }
}
