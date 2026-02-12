use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::Path;

use crate::config::ServiceConfig;
use crate::output::{self, LogLevel, Persistence};

use super::process::wait_for_restore_success;

const CHUNK_SIZE: usize = 8192;

pub(super) fn restore_from_file(
    service: &ServiceConfig,
    file_path: &Path,
    gzip: bool,
) -> Result<()> {
    let metadata = std::fs::metadata(file_path).context("SQL dump file not found")?;

    if metadata.len() == 0 {
        anyhow::bail!("SQL dump file is empty");
    }

    output::event(
        &service.name,
        LogLevel::Info,
        &format!("Restoring database from {}", file_path.display()),
        Persistence::Persistent,
    );

    let pb = ProgressBar::new(metadata.len());
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{bar:40}] {bytes}/{total_bytes} ({eta})")
            .context("Failed to create progress bar template")?
            .progress_chars("=>-"),
    );

    let mut child =
        crate::docker::exec_piped(service, false).context("Failed to start restore process")?;
    let mut stdin = child.stdin.take().context("Failed to open stdin pipe")?;

    let file = File::open(file_path).context("Failed to open SQL dump file")?;
    let use_gzip = gzip || is_gzip_path(file_path);

    let mut reader: Box<dyn Read> = if use_gzip {
        Box::new(GzDecoder::new(BufReader::new(file)))
    } else {
        Box::new(BufReader::new(file))
    };

    let mut buffer = [0_u8; CHUNK_SIZE];
    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .context("Failed to read from SQL dump file")?;

        if bytes_read == 0 {
            break;
        }

        let chunk = buffer
            .get(..bytes_read)
            .ok_or_else(|| anyhow::anyhow!("Buffer slice out of bounds"))?;

        stdin
            .write_all(chunk)
            .context("Failed to write to database process")?;

        pb.inc(u64::try_from(bytes_read).unwrap_or(0));
    }

    drop(stdin);
    let wait_result = wait_for_restore_success(child);
    pb.finish();

    wait_result?;
    output::event(
        &service.name,
        LogLevel::Success,
        "Database restored successfully",
        Persistence::Persistent,
    );
    Ok(())
}

fn is_gzip_path(path: &Path) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"))
}
