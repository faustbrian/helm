use anyhow::{Result, bail};

pub(super) fn validate_benchmark_project_count(
    expected: Option<usize>,
    actual: usize,
) -> Result<()> {
    let Some(expected) = expected else {
        return Ok(());
    };
    if expected != actual {
        bail!("benchmark expected exactly {expected} projects, but the daemon reported {actual}");
    }

    Ok(())
}
