//! Persistent file sink internals for logger state.

use std::collections::BTreeSet;
use std::fs::{File, OpenOptions, create_dir_all, read_dir, remove_file, rename};
use std::io::{Error, ErrorKind, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::output::entry::LogEntry;

pub(super) struct FileState {
    pub(super) directory: Option<PathBuf>,
    pub(super) day: Option<String>,
    pub(super) file: Option<File>,
    pub(super) bytes_written: u64,
}

const MAX_LOG_DAYS: usize = 7;
const MAX_LOG_FILE_BYTES: u64 = 10 * 1024 * 1024;

pub(super) fn default_log_dir_path() -> Option<PathBuf> {
    let Ok(home) = std::env::var("HOME") else {
        return None;
    };
    Some(PathBuf::from(home).join(".config/stackctl/logs"))
}

pub(super) fn persist_entry(file_state: &Mutex<FileState>, entry: &LogEntry) {
    let Ok(mut state) = file_state.lock() else {
        return;
    };
    let Some(directory) = state.directory.clone() else {
        return;
    };

    let day = entry.timestamp.date().to_string();
    ensure_log_file_for_day(&mut state, &directory, &day);

    write_entry(&mut state, &directory, &day, entry, MAX_LOG_FILE_BYTES);
}

fn ensure_log_file_for_day(state: &mut FileState, directory: &Path, day: &str) {
    if state.day.as_deref() == Some(day) && state.file.is_some() {
        return;
    }

    if prepare_log_directory(directory).is_err() {
        return;
    }
    prune_old_log_days(directory, day);
    let file_path = directory.join(format!("{day}.log"));
    let Ok(file) = open_private_log_file(&file_path) else {
        return;
    };
    state.bytes_written = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
    state.file = Some(file);
    state.day = Some(day.to_owned());
}

fn write_entry(
    state: &mut FileState,
    directory: &Path,
    day: &str,
    entry: &LogEntry,
    maximum_bytes: u64,
) {
    let line = entry.render_file_line();
    let line_bytes = u64::try_from(line.len().saturating_add(1)).unwrap_or(u64::MAX);
    if line_bytes > maximum_bytes {
        return;
    }
    if state.file.is_some()
        && state.bytes_written > 0
        && state.bytes_written.saturating_add(line_bytes) > maximum_bytes
    {
        rotate_log_file(state, directory, day);
    }
    let Some(file) = state.file.as_mut() else {
        return;
    };
    if writeln!(file, "{line}").is_ok() {
        state.bytes_written = state.bytes_written.saturating_add(line_bytes);
    }
}

fn rotate_log_file(state: &mut FileState, directory: &Path, day: &str) {
    state.file = None;
    let active = directory.join(format!("{day}.log"));
    let previous = directory.join(format!("{day}.log.1"));
    drop(remove_file(&previous));
    if rename(&active, &previous).is_err() {
        state.file = open_private_log_file(&active).ok();
        return;
    }
    state.file = create_private_log_file(&active).ok();
    state.bytes_written = 0;
}

fn prepare_log_directory(directory: &Path) -> Result<(), Error> {
    create_dir_all(directory)?;
    let metadata = std::fs::symlink_metadata(directory)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("'{}' is not a real log directory", directory.display()),
        ));
    }
    std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))
}

fn open_private_log_file(path: &Path) -> Result<File, Error> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
            OpenOptions::new().append(true).open(path)
        }
        Ok(_) => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("'{}' is not a real log file", path.display()),
        )),
        Err(error) if error.kind() == ErrorKind::NotFound => create_private_log_file(path),
        Err(error) => Err(error),
    }
}

fn create_private_log_file(path: &Path) -> Result<File, Error> {
    OpenOptions::new()
        .write(true)
        .append(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}

fn prune_old_log_days(directory: &Path, current_day: &str) {
    let mut days = BTreeSet::from([current_day.to_owned()]);
    let Ok(entries) = read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if let Some(day) = recognized_log_day(&name) {
            days.insert(day.to_owned());
        }
    }
    while days.len() > MAX_LOG_DAYS {
        let Some(oldest) = days.iter().find(|day| day.as_str() != current_day).cloned() else {
            break;
        };
        days.remove(&oldest);
        drop(remove_file(directory.join(format!("{oldest}.log"))));
        drop(remove_file(directory.join(format!("{oldest}.log.1"))));
    }
}

fn recognized_log_day(name: &str) -> Option<&str> {
    let day = name.get(..10)?;
    let suffix = name.get(10..)?;
    if !matches!(suffix, ".log" | ".log.1") {
        return None;
    }
    let bytes = day.as_bytes();
    (bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit()))
    .then_some(day)
}

#[cfg(test)]
mod tests {
    use super::{FileState, persist_entry, prune_old_log_days, write_entry};
    use crate::output::{LogLevel, Persistence, entry::LogEntry};
    use std::fs;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};
    use time::OffsetDateTime;

    #[cfg(unix)]
    #[test]
    fn persistent_logs_refuse_a_symbolic_link_directory() {
        use std::os::unix::fs::symlink;

        let directory = temporary_log_directory("linked-directory");
        let victim = directory.with_extension("victim");
        fs::create_dir_all(&victim).expect("create victim directory");
        symlink(&victim, &directory).expect("link log directory");
        let state = Mutex::new(FileState {
            directory: Some(directory.clone()),
            day: None,
            file: None,
            bytes_written: 0,
        });

        persist_entry(&state, &log_entry("linked directory"));

        assert_eq!(fs::read_dir(&victim).expect("read victim").count(), 0);
        fs::remove_file(directory).expect("remove directory link");
        fs::remove_dir_all(victim).expect("remove victim directory");
    }

    #[cfg(unix)]
    #[test]
    fn persistent_logs_refuse_a_symbolic_link_daily_file() {
        use std::os::unix::fs::symlink;

        let directory = temporary_log_directory("linked-file");
        let victim = directory.with_extension("victim");
        fs::create_dir_all(&directory).expect("create log directory");
        fs::write(&victim, "external\n").expect("write victim");
        symlink(&victim, directory.join("1970-01-01.log")).expect("link daily log");
        let state = Mutex::new(FileState {
            directory: Some(directory.clone()),
            day: None,
            file: None,
            bytes_written: 0,
        });

        persist_entry(&state, &log_entry("linked file"));

        assert_eq!(
            fs::read_to_string(&victim).expect("read victim"),
            "external\n"
        );
        fs::remove_dir_all(directory).expect("remove log directory");
        fs::remove_file(victim).expect("remove victim");
    }

    #[test]
    fn persistent_logs_retain_only_the_seven_newest_days() {
        let directory = temporary_log_directory("retention");
        fs::create_dir_all(&directory).expect("create logs");
        for day in 1..=8 {
            fs::write(directory.join(format!("2026-07-{day:02}.log")), "log\n")
                .expect("write daily log");
        }

        prune_old_log_days(&directory, "2026-07-08");

        assert!(!directory.join("2026-07-01.log").exists());
        for day in 2..=8 {
            assert!(directory.join(format!("2026-07-{day:02}.log")).exists());
        }
        fs::remove_dir_all(directory).expect("remove logs");
    }

    #[test]
    fn persistent_log_retention_never_deletes_the_active_clock_day() {
        let directory = temporary_log_directory("clock-day");
        fs::create_dir_all(&directory).expect("create logs");
        fs::write(directory.join("2026-07-01.log"), "active\n").expect("write active log");
        for day in 2..=8 {
            fs::write(directory.join(format!("2026-07-{day:02}.log")), "log\n")
                .expect("write future log");
        }

        prune_old_log_days(&directory, "2026-07-01");

        assert!(directory.join("2026-07-01.log").exists());
        assert_eq!(fs::read_dir(&directory).expect("read logs").count(), 7);
        fs::remove_dir_all(directory).expect("remove logs");
    }

    #[test]
    fn persistent_logs_keep_only_one_full_previous_size_segment() {
        let directory = temporary_log_directory("size");
        fs::create_dir_all(&directory).expect("create logs");
        let active = directory.join("2026-07-14.log");
        fs::write(&active, "first line\n").expect("write active log");
        let file = fs::OpenOptions::new()
            .append(true)
            .open(&active)
            .expect("open active log");
        let mut state = FileState {
            directory: Some(directory.clone()),
            day: Some("2026-07-14".to_owned()),
            file: Some(file),
            bytes_written: 95,
        };
        let entry = LogEntry {
            timestamp: OffsetDateTime::UNIX_EPOCH,
            level: LogLevel::Info,
            message: "newest line".to_owned(),
            context: None,
            persistence: Persistence::Persistent,
        };

        write_entry(&mut state, &directory, "2026-07-14", &entry, 100);

        assert_eq!(
            fs::read_to_string(directory.join("2026-07-14.log.1")).expect("previous segment"),
            "first line\n"
        );
        assert!(
            fs::read_to_string(&active)
                .expect("active segment")
                .contains("newest line")
        );
        fs::remove_dir_all(directory).expect("remove logs");
    }

    #[test]
    fn persistent_logs_reject_a_single_entry_larger_than_the_size_bound() {
        let directory = temporary_log_directory("oversized");
        fs::create_dir_all(&directory).expect("create logs");
        let active = directory.join("2026-07-14.log");
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&active)
            .expect("open active log");
        let mut state = FileState {
            directory: Some(directory.clone()),
            day: Some("2026-07-14".to_owned()),
            file: Some(file),
            bytes_written: 0,
        };
        let entry = LogEntry {
            timestamp: OffsetDateTime::UNIX_EPOCH,
            level: LogLevel::Info,
            message: "oversized entry".to_owned(),
            context: None,
            persistence: Persistence::Persistent,
        };

        write_entry(&mut state, &directory, "2026-07-14", &entry, 8);

        assert_eq!(fs::metadata(&active).expect("active metadata").len(), 0);
        fs::remove_dir_all(directory).expect("remove logs");
    }

    fn log_entry(message: &str) -> LogEntry {
        LogEntry {
            timestamp: OffsetDateTime::UNIX_EPOCH,
            level: LogLevel::Info,
            message: message.to_owned(),
            context: None,
            persistence: Persistence::Persistent,
        }
    }

    fn temporary_log_directory(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "stackctl-output-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ))
    }
}
