//! The reply loop: polling the server, end-of-file handling, passing data on for printing.
//!
//! May depend on: config, error, framing, output, session.

use std::io::Write;
use std::time::Duration;

use super::interrupt::Interrupt;
use crate::error::Error;
use crate::framing::FramingError;
use crate::session::{Recv, Session};
use crate::transport::Transport;

/// Reads wake up this often so that waiting for the server stays interruptible.
const POLL_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, PartialEq, Eq)]
pub enum ReplyEnd {
    Complete,
    /// The server closed the connection (or sent invalid framing) before any reply data.
    ConnectionClosed,
    /// Ctrl-C while waiting.
    Interrupted,
}

/// Writes the wire bytes recorded by the session trace, if tracing is on.
pub fn flush_trace<T: Transport>(
    session: &mut Session<T>,
    out: &mut dyn Write,
) -> Result<(), Error> {
    let trace = session.take_trace();
    if !trace.is_empty() {
        out.write_all(&trace)?;
        // Raw output is followed live, e.g. a stream of notifications.
        out.flush()?;
    }
    Ok(())
}

/// Reads one reply. With `stream`, data is written to `out` as it arrives; with `collect`,
/// it is also kept for the caller.
pub fn read_reply<T: Transport>(
    session: &mut Session<T>,
    out: &mut dyn Write,
    stream: bool,
    mut collect: Option<&mut Vec<u8>>,
    interrupt: &Interrupt,
) -> Result<ReplyEnd, Error> {
    let mut seen = 0usize;
    loop {
        if interrupt.triggered() {
            return Ok(ReplyEnd::Interrupted);
        }
        let received = session.recv(Some(POLL_INTERVAL));
        flush_trace(session, out)?;
        let (pending, error) = match received? {
            Recv::Timeout => continue,
            Recv::Data(data) => {
                seen += 1;
                if stream {
                    out.write_all(&data)?;
                }
                if let Some(buf) = collect.as_deref_mut() {
                    buf.extend_from_slice(&data);
                }
                continue;
            }
            Recv::EndOfMessage => return Ok(ReplyEnd::Complete),
            Recv::Eof(pending) => (pending, FramingError::UnexpectedEof),
            Recv::Framing(error, pending) => (pending, error),
        };
        if stream {
            out.write_all(&pending)?;
        }
        if let Some(buf) = collect.as_deref_mut() {
            buf.extend_from_slice(&pending);
        }
        return if seen == 0 {
            Ok(ReplyEnd::ConnectionClosed)
        } else {
            Err(error.into())
        };
    }
}
