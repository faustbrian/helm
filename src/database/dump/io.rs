use anyhow::{Context, Result};
use flate2::Compression;
use flate2::write::GzEncoder;
use std::fs::File;
use std::io::Write;
use std::path::Path;

pub(super) fn write_dump_file(path: &Path, bytes: &[u8], gzip: bool) -> Result<()> {
    if gzip {
        let file = File::create(path)
            .with_context(|| format!("Failed to create dump at {}", path.display()))?;
        let mut encoder = GzEncoder::new(file, Compression::default());
        encoder
            .write_all(bytes)
            .with_context(|| format!("Failed to write gzip dump to {}", path.display()))?;
        drop(encoder.finish()?);
        return Ok(());
    }

    std::fs::write(path, bytes)
        .with_context(|| format!("Failed to write dump to {}", path.display()))
}

pub(super) fn write_dump_stdout(bytes: &[u8], gzip: bool) -> Result<()> {
    if gzip {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(bytes).context("Failed to gzip dump")?;
        let compressed = encoder.finish().context("Failed to finalize gzip dump")?;
        std::io::stdout()
            .lock()
            .write_all(&compressed)
            .context("Failed to write gzip dump to stdout")?;
        return Ok(());
    }

    std::io::stdout()
        .lock()
        .write_all(bytes)
        .context("Failed to write dump to stdout")
}

pub(super) fn is_gzip_path(path: &Path) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"))
}
