use std::fs::{self, File};
use std::io::{Error, ErrorKind};
use std::path::Path;

/// Holds an exclusive OS lock on one existing real directory.
pub(crate) fn lock_directory(path: &Path) -> Result<File, Error> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("'{}' is not a real directory", path.display()),
        ));
    }
    let directory = File::open(path)?;
    directory.lock()?;

    Ok(directory)
}
