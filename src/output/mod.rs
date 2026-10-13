//! Reply printing for each output style.
//!
//! May depend on: config, error.

pub mod noaaa;
pub mod pretty;
pub mod schema;
pub mod xmllint;

use std::io::Write;

use crate::config::OutputStyle;
use crate::error::Error;

/// What to do with the next reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Read it and drop it.
    Discard,
    /// Write it to the output as it arrives.
    Stream,
    /// Collect it and write it formatted once it is complete.
    Collect,
}

/// Decides which replies are printed for an output style, and formats them.
#[derive(Debug, Clone, Copy)]
pub struct Printer {
    style: OutputStyle,
    /// Format with the external `xmllint` instead of the built-in formatter.
    external_xmllint: bool,
}

impl Printer {
    pub fn new(style: OutputStyle, external_xmllint: bool) -> Self {
        Printer {
            style,
            external_xmllint,
        }
    }

    /// `remaining` is the number of replies still to read, counting this one (negative: unbounded);
    /// `replies_seen` the number already read. Python's rules: raw prints nothing itself, plain and
    /// pretty/noaaa print the first reply, all prints every reply, last prints the final one.
    pub fn mode(&self, remaining: i64, replies_seen: usize) -> Mode {
        let first = replies_seen == 0;
        match self.style {
            OutputStyle::Raw => Mode::Discard,
            OutputStyle::Plain if first => Mode::Stream,
            OutputStyle::Pretty | OutputStyle::NoAaa if first => Mode::Collect,
            OutputStyle::Plain | OutputStyle::Pretty | OutputStyle::NoAaa => Mode::Discard,
            OutputStyle::All => Mode::Collect,
            OutputStyle::Last if remaining == 1 => Mode::Collect,
            OutputStyle::Last => Mode::Discard,
        }
    }

    /// Prints a reply that is already complete (the server hello).
    pub fn print_complete(
        &self,
        reply: &[u8],
        remaining: i64,
        replies_seen: usize,
        out: &mut dyn Write,
    ) -> Result<(), Error> {
        match self.mode(remaining, replies_seen) {
            Mode::Discard => Ok(()),
            Mode::Stream => Ok(out.write_all(reply)?),
            Mode::Collect => self.print_formatted(reply, out),
        }
    }

    /// Writes a complete reply formatted like `xmllint --format`, for the style.
    pub fn print_formatted(&self, reply: &[u8], out: &mut dyn Write) -> Result<(), Error> {
        let formatted = if self.external_xmllint {
            xmllint::format(reply)?
        } else {
            pretty::format(reply)?
        };
        let formatted = if self.style == OutputStyle::NoAaa {
            noaaa::filter(&formatted)
        } else {
            formatted
        };
        out.write_all(&formatted)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Mode::{Collect, Discard, Stream};

    fn modes(style: OutputStyle, remaining: &[i64]) -> Vec<Mode> {
        let printer = Printer::new(style, false);
        remaining
            .iter()
            .enumerate()
            .map(|(seen, left)| printer.mode(*left, seen))
            .collect()
    }

    #[test]
    fn which_replies_are_printed_per_style() {
        let three = [3, 2, 1];
        assert_eq!(modes(OutputStyle::Raw, &three), [Discard; 3]);
        assert_eq!(
            modes(OutputStyle::Plain, &three),
            [Stream, Discard, Discard]
        );
        assert_eq!(
            modes(OutputStyle::Pretty, &three),
            [Collect, Discard, Discard]
        );
        assert_eq!(
            modes(OutputStyle::NoAaa, &three),
            [Collect, Discard, Discard]
        );
        assert_eq!(modes(OutputStyle::All, &three), [Collect; 3]);
        assert_eq!(
            modes(OutputStyle::Last, &three),
            [Discard, Discard, Collect]
        );
    }

    #[test]
    fn last_never_prints_an_unbounded_stream() {
        assert_eq!(modes(OutputStyle::Last, &[-1, -1]), [Discard; 2]);
    }

    #[test]
    fn formatted_output_goes_through_the_style_filters() {
        let xml = b"<r><aaa><x/></aaa><b/></r>";
        let mut out = Vec::new();
        Printer::new(OutputStyle::Pretty, false)
            .print_formatted(xml, &mut out)
            .unwrap();
        assert!(String::from_utf8(out).unwrap().contains("<aaa>"));

        let mut out = Vec::new();
        Printer::new(OutputStyle::NoAaa, false)
            .print_formatted(xml, &mut out)
            .unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "<?xml version=\"1.0\"?>\n<r>\n  <b/>\n</r>\n"
        );
    }

    #[test]
    fn complete_replies_are_streamed_raw_or_formatted() {
        let mut out = Vec::new();
        Printer::new(OutputStyle::Plain, false)
            .print_complete(b"<a/>", 1, 0, &mut out)
            .unwrap();
        assert_eq!(out, b"<a/>");
        let mut out = Vec::new();
        Printer::new(OutputStyle::Raw, false)
            .print_complete(b"<a/>", 1, 0, &mut out)
            .unwrap();
        assert!(out.is_empty());
    }
}
