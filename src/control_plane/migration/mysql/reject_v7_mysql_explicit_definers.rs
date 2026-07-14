use crate::control_plane::migration::MigrationOperationError;
use std::path::Path;
use tokio::io::AsyncReadExt;

const BUFFER_BYTES: usize = 64 * 1024;
const DEFINER: &[u8] = b"DEFINER";

/// Refuses dumps that would import an accepted-v7 security principal into v8.
pub(super) async fn reject_v7_mysql_explicit_definers(
    artifact: &Path,
) -> Result<(), MigrationOperationError> {
    let mut file = tokio::fs::File::open(artifact).await.map_err(|error| {
        MigrationOperationError::new(format!(
            "open v7 MySQL-family dump for definer validation: {error}"
        ))
    })?;
    let mut scanner = ExplicitDefinerScanner::default();
    let mut buffer = vec![0_u8; BUFFER_BYTES];
    loop {
        let read = file.read(&mut buffer).await.map_err(|error| {
            MigrationOperationError::new(format!(
                "read v7 MySQL-family dump for definer validation: {error}"
            ))
        })?;
        if read == 0 {
            break;
        }
        if scanner.scan(&buffer[..read]) {
            return Err(MigrationOperationError::new(
                "v7 MySQL-family dump contains an explicit DEFINER identity; refusing to import a legacy security principal into a shared v8 instance",
            ));
        }
    }

    Ok(())
}

#[derive(Default)]
struct ExplicitDefinerScanner {
    matched: usize,
    awaiting_equals: bool,
}

impl ExplicitDefinerScanner {
    fn scan(&mut self, bytes: &[u8]) -> bool {
        for byte in bytes.iter().copied() {
            if self.awaiting_equals {
                if byte == b'=' {
                    return true;
                }
                if byte.is_ascii_whitespace() {
                    continue;
                }
                self.awaiting_equals = false;
                self.matched = usize::from(byte.eq_ignore_ascii_case(&DEFINER[0]));
                continue;
            }

            if byte.eq_ignore_ascii_case(&DEFINER[self.matched]) {
                self.matched += 1;
                if self.matched == DEFINER.len() {
                    self.matched = 0;
                    self.awaiting_equals = true;
                }
            } else {
                self.matched = usize::from(byte.eq_ignore_ascii_case(&DEFINER[0]));
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::ExplicitDefinerScanner;

    #[test]
    fn explicit_definer_scanner_is_case_insensitive_and_cross_chunk() {
        let mut scanner = ExplicitDefinerScanner::default();

        assert!(!scanner.scan(b"CREATE def"));
        assert!(!scanner.scan(b"iner \n"));
        assert!(scanner.scan(b"= `legacy`@`%` VIEW"));
    }
}
