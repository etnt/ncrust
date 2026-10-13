//! Library behind the `netconf-console` binary.
//!
//! Module dependency rules are enforced by `tests/architecture.rs`.

pub mod cli;
pub mod config;
pub mod error;
pub mod framing;
pub mod input;
pub mod output;
pub mod protocol;
pub mod runner;
pub mod session;
pub mod transport;

use std::io::Write;

use error::Error;

/// Runs the tool with `args` (without the program name); normal output goes to `out`.
pub fn run(args: &[String], out: &mut impl Write) -> Result<(), Error> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        writeln!(out, "Usage: netconf-console [options] [<filename> | -]")?;
        return Ok(());
    }
    Err(Error::Stderr("netconf-console: not implemented yet".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_prints_usage() {
        let mut out = Vec::new();
        run(&["--help".to_string()], &mut out).unwrap();
        assert!(String::from_utf8(out).unwrap().starts_with("Usage:"));
    }

    #[test]
    fn anything_else_is_not_implemented_yet() {
        let mut out = Vec::new();
        let err = run(&[], &mut out).unwrap_err();
        assert!(matches!(err, Error::Stderr(_)));
        assert!(out.is_empty());
    }
}
