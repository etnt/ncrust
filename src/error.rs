//! Error type shared by all modules, and the rule for how an error is reported.
//!
//! May depend on: nothing.

use std::io::{self, Write};

/// Which output stream an error message belongs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stream {
    Stdout,
    Stderr,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Failure reported on stdout (Python `StdoutError`: connect and authentication failures, bad options).
    #[error("{0}")]
    Stdout(String),
    /// Failure reported on stderr (Python `StderrError`: protocol and formatting failures).
    #[error("{0}")]
    Stderr(String),
    #[error("{0}")]
    Io(#[from] io::Error),
}

impl Error {
    pub fn stream(&self) -> Stream {
        match self {
            Error::Stdout(_) => Stream::Stdout,
            Error::Stderr(_) | Error::Io(_) => Stream::Stderr,
        }
    }

    /// All failures exit with 1, as the Python script does.
    pub fn exit_code(&self) -> u8 {
        1
    }

    pub fn is_broken_pipe(&self) -> bool {
        matches!(self, Error::Io(e) if e.kind() == io::ErrorKind::BrokenPipe)
    }
}

/// Prints `err` on its stream (nothing for a broken pipe) and returns the exit code.
pub fn report(err: &Error, stdout: &mut impl Write, stderr: &mut impl Write) -> u8 {
    if !err.is_broken_pipe() {
        // Best effort: if the stream is broken too there is nowhere left to report to.
        let _ = match err.stream() {
            Stream::Stdout => writeln!(stdout, "{err}"),
            Stream::Stderr => writeln!(stderr, "{err}"),
        };
    }
    err.exit_code()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_report(err: &Error) -> (u8, String, String) {
        let (mut out, mut errout) = (Vec::new(), Vec::new());
        let code = report(err, &mut out, &mut errout);
        (
            code,
            String::from_utf8(out).unwrap(),
            String::from_utf8(errout).unwrap(),
        )
    }

    #[test]
    fn stdout_error_goes_to_stdout() {
        let (code, out, err) = run_report(&Error::Stdout("Authentication failed.".into()));
        assert_eq!(
            (code, out.as_str(), err.as_str()),
            (1, "Authentication failed.\n", "")
        );
    }

    #[test]
    fn stderr_error_goes_to_stderr() {
        let (code, out, err) = run_report(&Error::Stderr("No data received in the reply.".into()));
        assert_eq!(
            (code, out.as_str(), err.as_str()),
            (1, "", "No data received in the reply.\n")
        );
    }

    #[test]
    fn other_io_error_goes_to_stderr() {
        let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        let (code, out, err) = run_report(&Error::from(io_err));
        assert_eq!((code, out.as_str(), err.as_str()), (1, "", "denied\n"));
    }

    #[test]
    fn broken_pipe_is_silent() {
        let (code, out, err) = run_report(&Error::from(io::Error::from(io::ErrorKind::BrokenPipe)));
        assert_eq!((code, out.as_str(), err.as_str()), (1, "", ""));
    }
}
