//! swarm target exec io module.
//!
//! Contains swarm target exec io logic used by Helm command workflows.

use std::io::{BufRead, BufReader, Read};
use std::process::{ChildStderr, ChildStdout};

use super::OutputMode;
use crate::output::{self, Channel, Persistence};

pub(super) fn spawn_prefixed_output_threads(
    target_name: &str,
    stdout: ChildStdout,
    stderr: ChildStderr,
    output_mode: OutputMode,
) -> (std::thread::JoinHandle<()>, std::thread::JoinHandle<()>) {
    let stdout_thread = spawn_stream_thread(
        target_name.to_owned(),
        stdout,
        Channel::Out,
        output_mode,
        false,
    );
    let stderr_thread = spawn_stream_thread(
        target_name.to_owned(),
        stderr,
        Channel::Err,
        output_mode,
        true,
    );

    (stdout_thread, stderr_thread)
}

fn spawn_stream_thread<R>(
    target_name: String,
    stream: R,
    channel: Channel,
    output_mode: OutputMode,
    passthrough_stderr: bool,
) -> std::thread::JoinHandle<()>
where
    R: Read + Send + 'static,
{
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => emit_stream_line(
                    &target_name,
                    channel,
                    line.trim_end_matches('\n').trim_end_matches('\r'),
                    output_mode,
                    passthrough_stderr,
                ),
                Err(_) => break,
            }
        }
    })
}

fn emit_stream_line(
    target_name: &str,
    channel: Channel,
    rendered: &str,
    output_mode: OutputMode,
    passthrough_stderr: bool,
) {
    match output_mode {
        OutputMode::Logged => {
            output::stream(target_name, channel, rendered, Persistence::Persistent)
        }
        OutputMode::Passthrough => {
            if passthrough_stderr {
                eprintln!("{rendered}");
            } else {
                println!("{rendered}");
            }
        }
    }
}
