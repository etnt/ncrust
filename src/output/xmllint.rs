//! Formatting through the external `xmllint` program (escape hatch for the built-in formatter).
//!
//! May depend on: error.

use std::io::Write;
use std::process::{Command, Stdio};

use crate::error::Error;

/// `xmllint --format -` applied to `xml`.
pub fn format(xml: &[u8]) -> Result<Vec<u8>, Error> {
    let missing = || Error::Stderr("Problem with xmllint executable. Is it in PATH?".into());
    let mut child = Command::new("xmllint")
        .args(["--format", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| missing())?;

    // Feed the input from another thread so that a large reply cannot fill both pipes at once.
    let mut stdin = child.stdin.take().ok_or_else(missing)?;
    let input = xml.to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let output = child.wait_with_output().map_err(|_| missing())?;
    // xmllint may stop reading early on malformed input; its exit status tells.
    let _ = writer.join();

    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(Error::Stderr(
            "xmllint failed. Is the returned XML ok?".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn xmllint_available() -> bool {
        Command::new("xmllint").arg("--version").output().is_ok()
    }

    #[test]
    fn formats_through_xmllint_when_it_is_installed() {
        if !xmllint_available() {
            eprintln!("xmllint not installed, skipping");
            return;
        }
        let out = format(b"<a><b/></a>").unwrap();
        assert_eq!(out, b"<?xml version=\"1.0\"?>\n<a>\n  <b/>\n</a>\n");
    }

    #[test]
    fn malformed_input_is_reported_like_python() {
        if !xmllint_available() {
            return;
        }
        let err = format(b"<a><b></a>").unwrap_err();
        assert_eq!(err.to_string(), "xmllint failed. Is the returned XML ok?");
    }
}
