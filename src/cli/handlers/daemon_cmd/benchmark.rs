use anyhow::{Result, bail};

#[cfg(unix)]
pub(super) fn handle_daemon_benchmark() -> Result<()> {
    use crate::control_plane::{IpcOutcome, IpcPayload, IpcResult};
    use std::io::Write as _;

    let response = super::send_singleton_request(IpcPayload::BenchmarkSnapshot)?;
    match response.outcome() {
        IpcOutcome::Success {
            result: IpcResult::BenchmarkSnapshot { snapshot },
        } => {
            let stdout = std::io::stdout();
            let mut output = stdout.lock();
            serde_json::to_writer_pretty(&mut output, snapshot)?;
            output.write_all(b"\n")?;
            output.flush()?;

            Ok(())
        }
        IpcOutcome::Success { .. } => bail!("daemon returned an unexpected benchmark response"),
        IpcOutcome::Failure { diagnostics } => {
            let message = diagnostics
                .iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.code(), diagnostic.message()))
                .collect::<Vec<_>>()
                .join("; ");
            bail!("benchmark snapshot failed: {message}")
        }
    }
}
