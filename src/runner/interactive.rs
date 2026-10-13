//! Interactive mode: read operations from stdin until an empty line.
//!
//! May depend on: error, session.

use std::io::{BufRead, Write};

use crate::error::Error;
use crate::protocol::ns::NC_NS;

const PROMPT: &str =
    "\n* Enter a NETCONF operation, end with an empty line, or only empty continuing session\n";

/// Prompts for one operation and wraps it in `<rpc>`. `None` means an empty line or the end
/// of the input: nothing is sent and the session just keeps listening to the server.
pub fn read_request(
    input: &mut dyn BufRead,
    out: &mut dyn Write,
    message_id: u32,
) -> Result<Option<String>, Error> {
    out.write_all(PROMPT.as_bytes())?;
    out.flush()?;

    let mut line = String::new();
    if input.read_line(&mut line)? == 0 || line == "\n" {
        return Ok(None);
    }
    let mut message = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rpc xmlns=\"{NC_NS}\" message-id=\"{message_id}\">\n"
    );
    while !line.is_empty() && line != "\n" {
        message.push_str(&line);
        line.clear();
        input.read_line(&mut line)?;
    }
    message.push_str("</rpc>\n");
    Ok(Some(message))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(input: &str, id: u32) -> (Option<String>, String) {
        let mut out = Vec::new();
        let request = read_request(&mut input.as_bytes(), &mut out, id).unwrap();
        (request, String::from_utf8(out).unwrap())
    }

    #[test]
    fn wraps_the_lines_up_to_the_empty_line() {
        let (request, out) = read("<get>\n</get>\n\nrest", 5);
        assert_eq!(
            request.unwrap(),
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rpc xmlns=\"{NC_NS}\" message-id=\"5\">\n<get>\n</get>\n</rpc>\n"
            )
        );
        assert!(
            out.starts_with("\n* Enter a NETCONF operation, end with an empty line"),
            "{out}"
        );
    }

    #[test]
    fn an_empty_line_or_end_of_input_sends_nothing() {
        assert_eq!(read("\n<get/>\n", 2).0, None);
        assert_eq!(read("", 2).0, None);
    }

    #[test]
    fn end_of_input_ends_the_operation() {
        let request = read("<get/>", 2).0.unwrap();
        assert!(request.ends_with("<get/></rpc>\n"), "{request}");
    }
}
