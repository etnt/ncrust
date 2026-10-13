//! Filter that removes Tail-f AAA and NACM subtrees from formatted output.
//!
//! May depend on: nothing.

/// Applies the legacy awk filter of the Python script to formatted output, line by line:
/// a line containing `<aaa` or `<nacm` starts dropping lines, a line containing `</aaa>` or
/// `</nacm>` ends it (and is dropped too). The quirks are kept: a self-closing `<aaa/>` starts
/// dropping and nothing ends it.
pub fn filter(formatted: &[u8]) -> Vec<u8> {
    fn contains(line: &[u8], needle: &[u8]) -> bool {
        line.windows(needle.len()).any(|w| w == needle)
    }

    let mut lines: Vec<&[u8]> = formatted.split(|&b| b == b'\n').collect();
    // A trailing newline does not start another record.
    if lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }

    let mut out = Vec::with_capacity(formatted.len());
    let mut deleting = false;
    for line in lines {
        if contains(line, b"<aaa") || contains(line, b"<nacm") {
            deleting = true;
        }
        if contains(line, b"</aaa>") || contains(line, b"</nacm>") {
            deleting = false;
            continue;
        }
        if deleting {
            continue;
        }
        out.extend_from_slice(line);
        out.push(b'\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(input: &str) -> String {
        String::from_utf8(filter(input.as_bytes())).unwrap()
    }

    #[test]
    fn drops_aaa_and_nacm_blocks() {
        let input = "<a>\n  <aaa xmlns=\"x\">\n    <b/>\n  </aaa>\n  <c/>\n  <nacm:nacm>\n  </nacm>\n</a>\n";
        assert_eq!(run(input), "<a>\n  <c/>\n</a>\n");
    }

    #[test]
    fn a_start_and_end_on_one_line_drops_only_that_line() {
        assert_eq!(run("x\n<aaa/></aaa>\ny\n"), "x\ny\n");
    }

    #[test]
    fn a_self_closing_start_drops_everything_after_it() {
        assert_eq!(run("x\n<aaa/>\ny\nz\n"), "x\n");
    }

    #[test]
    fn prefix_matches_like_the_awk_regex() {
        assert_eq!(run("<aaaa>\nq\n</aaa>\nr\n"), "r\n");
    }

    #[test]
    fn input_without_trailing_newline_gets_one() {
        assert_eq!(run("a\nb"), "a\nb\n");
        assert_eq!(run(""), "");
        assert_eq!(run("a\n\n"), "a\n\n");
    }
}
