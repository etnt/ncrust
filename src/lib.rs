//! Library behind the `ncrust` binary.
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

/// Runs the tool with `args` (without the program name). Normal output goes to `out`;
/// notes about unexpected replies go to `err`.
pub fn run(args: &[String], out: &mut impl Write, err: &mut impl Write) -> Result<(), Error> {
    match cli::parse(args)? {
        cli::Parsed::Help(text) => {
            out.write_all(text.as_bytes())?;
            Ok(())
        }
        cli::Parsed::Run(invocation) => runner::run(&invocation, out, err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_prints_usage() {
        let mut out = Vec::new();
        run(&["--help".to_string()], &mut out, &mut Vec::new()).unwrap();
        assert!(String::from_utf8(out).unwrap().starts_with("Usage:"));
    }

    #[test]
    fn missing_command_is_a_usage_error() {
        let mut out = Vec::new();
        let err = run(&[], &mut out, &mut Vec::new()).unwrap_err();
        assert!(matches!(err, Error::Usage(_)));
        assert!(out.is_empty());
    }
}
