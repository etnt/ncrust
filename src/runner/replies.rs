//! The reply loop: polling, end-of-file handling, printing per output style.
//!
//! May depend on: config, error, output, session.

use std::io::Write;
use std::time::Duration;

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
}

/// Writes the wire bytes recorded by the session trace, if tracing is on.
pub fn flush_trace<T: Transport>(
    session: &mut Session<T>,
    out: &mut dyn Write,
) -> Result<(), Error> {
    out.write_all(&session.take_trace())?;
    Ok(())
}

/// Reads one reply, writing its text to `out` if `print` is set.
pub fn read_reply<T: Transport>(
    session: &mut Session<T>,
    out: &mut dyn Write,
    print: bool,
) -> Result<ReplyEnd, Error> {
    let mut seen = 0usize;
    loop {
        let received = session.recv(Some(POLL_INTERVAL));
        flush_trace(session, out)?;
        let (pending, error) = match received? {
            Recv::Timeout => continue,
            Recv::Data(data) => {
                seen += 1;
                if print {
                    out.write_all(&data)?;
                }
                continue;
            }
            Recv::EndOfMessage => return Ok(ReplyEnd::Complete),
            Recv::Eof(pending) => (pending, FramingError::UnexpectedEof),
            Recv::Framing(error, pending) => (pending, error),
        };
        if print {
            out.write_all(&pending)?;
        }
        return if seen == 0 {
            Ok(ReplyEnd::ConnectionClosed)
        } else {
            Err(error.into())
        };
    }
}
