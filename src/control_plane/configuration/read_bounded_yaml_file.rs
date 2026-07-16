use std::fs;
use std::io::{self, Read};
use std::path::Path;

use rustix::fs::{FileType, Mode, OFlags, fstat, open};

/// Reads one regular non-symlink YAML file without crossing its byte budget.
pub(crate) fn read_bounded_yaml_file(path: &Path, maximum: usize) -> io::Result<String> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "YAML file must not be a symbolic link",
        ));
    }
    let descriptor = open(
        path,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )?;
    let opened_metadata = fstat(&descriptor)?;
    if FileType::from_raw_mode(opened_metadata.st_mode) != FileType::RegularFile {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "YAML path must be a regular file",
        ));
    }
    let opened_size = u64::try_from(opened_metadata.st_size).unwrap_or(u64::MAX);
    if opened_size > u64::try_from(maximum).unwrap_or(u64::MAX) {
        return Err(oversized_error(opened_size, maximum));
    }

    let file = fs::File::from(descriptor);
    let limit = u64::try_from(maximum).unwrap_or(u64::MAX).saturating_add(1);
    let mut bytes = Vec::with_capacity(maximum.min(64 * 1024));
    file.take(limit).read_to_end(&mut bytes)?;
    if bytes.len() > maximum {
        return Err(oversized_error(
            u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            maximum,
        ));
    }

    String::from_utf8(bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.utf8_error()))
}

fn oversized_error(actual: u64, maximum: usize) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("YAML file is {actual} bytes; maximum is {maximum} bytes"),
    )
}

#[cfg(test)]
mod tests {
    use super::read_bounded_yaml_file;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn bounded_reader_rejects_oversized_and_invalid_utf8_files() {
        let root = temporary_directory("bounded-yaml");
        let oversized = root.join("oversized.yaml");
        fs::write(&oversized, b"12345").expect("write oversized YAML");
        let oversized_error = read_bounded_yaml_file(&oversized, 4).expect_err("size boundary");
        assert!(
            oversized_error
                .to_string()
                .contains("5 bytes; maximum is 4 bytes")
        );

        let invalid = root.join("invalid.yaml");
        fs::write(&invalid, [0xff]).expect("write invalid YAML");
        let invalid_error = read_bounded_yaml_file(&invalid, 4).expect_err("UTF-8 boundary");
        assert_eq!(invalid_error.kind(), std::io::ErrorKind::InvalidData);

        fs::remove_dir_all(root).expect("remove bounded YAML fixture");
    }

    #[cfg(unix)]
    #[test]
    fn bounded_reader_rejects_symbolic_links() {
        use std::os::unix::fs::symlink;

        let root = temporary_directory("symlink-yaml");
        let target = root.join("target.yaml");
        let link = root.join("link.yaml");
        fs::write(&target, "services: {}\n").expect("write target YAML");
        symlink(&target, &link).expect("link YAML");

        let error = read_bounded_yaml_file(&link, 1024).expect_err("symlink boundary");

        assert!(error.to_string().contains("must not be a symbolic link"));
        fs::remove_dir_all(root).expect("remove symlink YAML fixture");
    }

    fn temporary_directory(name: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("stackctl-{name}-{}-{unique}", std::process::id()));
        fs::create_dir(&path).expect("create YAML fixture");
        path
    }
}
