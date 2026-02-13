//! swarm target exec io module.
//!
//! Contains swarm target exec io logic used by Helm command workflows.

use std::io::{BufRead, BufReader};
use std::process::ChildStderr;
use std::process::ChildStdout;

use super::OutputMode;
use crate::output::{self, Channel, Persistence};

pub(super) fn spawn_prefixed_output_threads(
    target_name: &str,
    stdout: ChildStdout,
    stderr: ChildStderr,
    output_mode: OutputMode,
) -> (std::thread::JoinHandle<()>, std::thread::JoinHandle<()>) {
    let stdout_name = target_name.to_owned();
    let stderr_name = target_name.to_owned();
    let stdout_mode = output_mode;
    let stderr_mode = output_mode;

    let stdout_thread = std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        loop {
            line.clear();
            let read = reader.read_line(&mut line);
            match read {
                Ok(0) => break,
                Ok(_) => {
                    let rendered = line.trim_end_matches('\n').trim_end_matches('\r');
                    match stdout_mode {
                        OutputMode::Logged => output::stream(
                            &stdout_name,
                            Channel::Out,
                            rendered,
                            Persistence::Persistent,
                        ),
                        OutputMode::Passthrough => println!("{rendered}"),
                    }
                }
                Err(_) => break,
            }
        }
    });

    let stderr_thread = std::thread::spawn(move || {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        loop {
            line.clear();
            let read = reader.read_line(&mut line);
            match read {
                Ok(0) => break,
                Ok(_) => {
                    let rendered = line.trim_end_matches('\n').trim_end_matches('\r');
                    match stderr_mode {
                        OutputMode::Logged => output::stream(
                            &stderr_name,
                            Channel::Err,
                            rendered,
                            Persistence::Persistent,
                        ),
                        OutputMode::Passthrough => eprintln!("{rendered}"),
                    }
                }
                Err(_) => break,
            }
        }
    });

    (stdout_thread, stderr_thread)
}
