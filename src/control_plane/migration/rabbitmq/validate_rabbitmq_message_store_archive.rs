use crate::control_plane::migration::MigrationOperationError;
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path};

/// Proves every stored tar entry stays below one declared vhost-store root.
pub(super) fn validate_rabbitmq_message_store_archive(
    mut artifact_file: File,
    archive_offset: u64,
    relative_message_store_path: &Path,
    expected_sha256: &str,
    expected_size_bytes: u64,
) -> Result<File, MigrationOperationError> {
    let expected_root = relative_message_store_path.file_name().ok_or_else(|| {
        MigrationOperationError::new(
            "RabbitMQ recovery message-store path has no terminal directory",
        )
    })?;
    artifact_file
        .seek(SeekFrom::Start(0))
        .map_err(|error| operation_error("RabbitMQ recovery artifact seek failed", error))?;
    let mut digest = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = artifact_file
            .read(&mut buffer)
            .map_err(|error| operation_error("RabbitMQ recovery artifact read failed", error))?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
        size = size.saturating_add(u64::try_from(count).unwrap_or(u64::MAX));
    }
    if hex::encode(digest.finalize()) != expected_sha256 || size != expected_size_bytes {
        return Err(MigrationOperationError::new(
            "RabbitMQ recovery artifact changed after verification",
        ));
    }
    artifact_file
        .seek(SeekFrom::Start(archive_offset))
        .map_err(|error| {
            operation_error("RabbitMQ recovery message-store archive seek failed", error)
        })?;
    {
        let mut archive = tar::Archive::new(&mut artifact_file);
        validate_entries(&mut archive, expected_root)?;
    }
    artifact_file
        .seek(SeekFrom::Start(archive_offset))
        .map_err(|error| {
            operation_error(
                "RabbitMQ recovery message-store archive rewind failed",
                error,
            )
        })?;

    Ok(artifact_file)
}

fn validate_entries<R: Read>(
    archive: &mut tar::Archive<R>,
    expected_root: &OsStr,
) -> Result<(), MigrationOperationError> {
    let entries = archive.entries().map_err(|error| {
        operation_error(
            "RabbitMQ recovery message-store archive enumeration failed",
            error,
        )
    })?;
    let mut entry_count = 0_u64;
    for entry in entries {
        let entry = entry.map_err(|error| {
            operation_error(
                "RabbitMQ recovery message-store tar entry is invalid",
                error,
            )
        })?;
        let path = entry.path().map_err(|error| {
            operation_error("RabbitMQ recovery message-store tar path is invalid", error)
        })?;
        let mut components = path.components();
        let safe_root =
            matches!(components.next(), Some(Component::Normal(root)) if root == expected_root);
        let safe_children = components.all(|component| matches!(component, Component::Normal(_)));
        let entry_type = entry.header().entry_type();
        let safe_type = entry_type.is_file() || entry_type.is_dir();
        let has_link = entry
            .link_name()
            .map_err(|error| {
                operation_error("RabbitMQ recovery message-store tar link is invalid", error)
            })?
            .is_some();
        if !safe_root || !safe_children || !safe_type || has_link {
            return Err(MigrationOperationError::new(format!(
                "RabbitMQ recovery message-store tar entry '{}' escapes or aliases the declared vhost store",
                path.display()
            )));
        }
        entry_count = entry_count.checked_add(1).ok_or_else(|| {
            MigrationOperationError::new(
                "RabbitMQ recovery message-store archive entry count overflowed",
            )
        })?;
    }
    if entry_count == 0 {
        return Err(MigrationOperationError::new(
            "RabbitMQ recovery message-store archive is empty",
        ));
    }

    Ok(())
}

fn operation_error(context: &str, error: impl std::fmt::Display) -> MigrationOperationError {
    MigrationOperationError::new(format!("{context}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::validate_entries;
    use std::ffi::OsStr;
    use std::io::Cursor;

    #[test]
    fn message_store_archive_rejects_a_sibling_vhost_entry() {
        let archive = archive_with_file("FOREIGN/msg_store_persistent", b"message");
        let mut archive = tar::Archive::new(Cursor::new(archive));

        let error = validate_entries(&mut archive, OsStr::new("628Q7P"))
            .expect_err("sibling vhost entry must fail closed");

        assert!(error.to_string().contains("escapes or aliases"));
    }

    #[test]
    fn message_store_archive_accepts_only_the_declared_vhost_subtree() {
        let archive = archive_with_file("628Q7P/msg_store_persistent", b"message");
        let mut archive = tar::Archive::new(Cursor::new(archive));

        validate_entries(&mut archive, OsStr::new("628Q7P")).expect("declared vhost subtree");
    }

    #[test]
    fn message_store_archive_rejects_links_inside_the_declared_subtree() {
        let mut bytes = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut bytes);
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_mode(0o700);
            header.set_size(0);
            header
                .set_link_name("../FOREIGN")
                .expect("unsafe link target");
            header.set_cksum();
            builder
                .append_data(&mut header, "628Q7P/alias", std::io::empty())
                .expect("symlink tar entry");
            builder.finish().expect("finish symlink tar");
        }
        let mut archive = tar::Archive::new(Cursor::new(bytes));

        let error = validate_entries(&mut archive, OsStr::new("628Q7P"))
            .expect_err("tar links must fail closed");

        assert!(error.to_string().contains("escapes or aliases"));
    }

    fn archive_with_file(path: &str, contents: &[u8]) -> Vec<u8> {
        let mut archive = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut archive);
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Regular);
            header.set_mode(0o600);
            header.set_size(contents.len() as u64);
            header.set_cksum();
            builder
                .append_data(&mut header, path, contents)
                .expect("tar entry");
            builder.finish().expect("finish tar");
        }

        archive
    }
}
