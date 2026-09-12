//! Private startup handshake between `start` and the background service.

use std::io::{self, Write as _};

use tokio::io::{AsyncBufReadExt as _, AsyncRead, BufReader};

const READY: &str = "WF_OBSERVER_AGENT_READY";
const ERROR_PREFIX: &str = "WF_OBSERVER_AGENT_ERROR=";

/// The background agent's startup result.
#[derive(Debug, ..Eq)]
pub(crate) enum Status {
    /// Initialization completed and the agent owns the singleton lock.
    Ready,
    /// Initialization failed before the agent became ready.
    Failed(String),
}

/// Reports successful initialization to the invoking CLI.
pub(crate) fn report_ready() -> io::Result<()> {
    write_line(READY)
}

/// Sends an initialization failure to the invoking CLI.
pub(crate) fn send_failure(error: &anyhow::Error) -> io::Result<()> {
    let message = format!("{error:#}").replace(['\r', '\n'], " ");
    write_line(&format!("{ERROR_PREFIX}{message}"))
}

/// Reads the first startup result emitted by the agent.
pub(crate) async fn read(reader: impl AsyncRead + Unpin) -> io::Result<Status> {
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    if reader.read_line(&mut line).await? == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "agent exited without reporting its startup status",
        ));
    }

    parse(line.trim_end_matches(['\r', '\n']))
}

fn write_line(line: &str) -> io::Result<()> {
    let stdout = io::stdout();
    let mut stdout = stdout.lock();
    writeln!(stdout, "{line}")?;
    stdout.flush()
}

fn parse(line: &str) -> io::Result<Status> {
    if line == READY {
        return Ok(Status::Ready);
    }

    if let Some(message) = line.strip_prefix(ERROR_PREFIX) {
        return Ok(Status::Failed(message.to_owned()));
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!("agent reported an invalid startup status: {line}"),
    ))
}
