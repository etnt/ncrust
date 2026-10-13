//! Formatter that reproduces `xmllint --format`.
//!
//! May depend on: error.
//!
//! The rules come from libxml2 (2.9): whitespace-only text between elements is dropped unless
//! the element has (or gets) real text, and an element with a text child is written without
//! any indentation inside. `tests/pretty_golden.rs` and `tests/pretty_vs_xmllint.rs` compare
//! the result with the real program.

use crate::error::Error;

/// libxml2 refuses deeper documents unless asked to.
const MAX_DEPTH: usize = 256;
/// libxml2 stops indenting deeper than this many levels (2 spaces each).
const MAX_INDENT_LEVELS: usize = 100;

/// Formats `xml` like `xmllint --format`.
pub fn format(xml: &[u8]) -> Result<Vec<u8>, Error> {
    try_format(xml)
        .map(String::into_bytes)
        .map_err(|_| Error::Stderr("xmllint failed. Is the returned XML ok?".into()))
}

/// Why a document could not be formatted (for tests and debugging).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError(pub String);

pub fn try_format(xml: &[u8]) -> Result<String, ParseError> {
    let text = std::str::from_utf8(xml).map_err(|_| ParseError("not valid UTF-8".into()))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    // XML line-end normalisation: CRLF and lone CR become LF.
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let doc = Parser { s: &text, pos: 0 }.document()?;
    Ok(serialize(&doc))
}

#[derive(Debug)]
enum Node {
    Element(Element),
    Text(String),
    CData(String),
    Comment(String),
    Pi { target: String, data: String },
}

#[derive(Debug)]
struct Element {
    name: String,
    /// `xmlns` and `xmlns:*` attributes; libxml2 writes them before the other attributes.
    ns_decls: Vec<(String, String)>,
    attrs: Vec<(String, String)>,
    children: Vec<Node>,
}

#[derive(Debug)]
struct Document {
    version: String,
    encoding: Option<String>,
    standalone: Option<bool>,
    /// Comments, processing instructions and the root element.
    children: Vec<Node>,
}

struct Parser<'a> {
    s: &'a str,
    pos: usize,
}

type ParseResult<T> = std::result::Result<T, ParseError>;

fn is_xml_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')
}

impl<'a> Parser<'a> {
    fn rest(&self) -> &'a str {
        &self.s[self.pos..]
    }

    fn eof(&self) -> bool {
        self.pos >= self.s.len()
    }

    fn starts_with(&self, prefix: &str) -> bool {
        self.rest().starts_with(prefix)
    }

    fn eat(&mut self, prefix: &str) -> bool {
        let found = self.starts_with(prefix);
        if found {
            self.pos += prefix.len();
        }
        found
    }

    fn fail<T>(&self, message: &str) -> ParseResult<T> {
        Err(ParseError(format!("{message} at byte {}", self.pos)))
    }

    /// Skips XML whitespace; true if there was any.
    fn skip_space(&mut self) -> bool {
        let rest = self.rest();
        let trimmed = rest.trim_start_matches(is_xml_space);
        self.pos += rest.len() - trimmed.len();
        trimmed.len() != rest.len()
    }

    fn name(&mut self) -> ParseResult<String> {
        let rest = self.rest();
        let len = rest
            .find(|c: char| {
                is_xml_space(c) || matches!(c, '/' | '>' | '=' | '<' | '&' | '\'' | '"' | '?')
            })
            .unwrap_or(rest.len());
        if len == 0 {
            return self.fail("expected a name");
        }
        self.pos += len;
        Ok(rest[..len].to_string())
    }

    /// Text up to `end`, consuming `end`.
    fn until(&mut self, end: &str, what: &str) -> ParseResult<&'a str> {
        let rest = self.rest();
        let Some(i) = rest.find(end) else {
            return self.fail(&format!("unterminated {what}"));
        };
        self.pos += i + end.len();
        Ok(&rest[..i])
    }

    fn document(mut self) -> ParseResult<Document> {
        let mut doc = Document {
            version: "1.0".into(),
            encoding: None,
            standalone: None,
            children: Vec::new(),
        };
        if self.starts_with("<?xml") && self.s[5..].starts_with(is_xml_space) {
            self.pos += 5;
            self.xml_declaration(&mut doc)?;
        }
        let mut root_seen = false;
        loop {
            self.skip_space();
            if self.eof() {
                break;
            }
            if self.eat("<!--") {
                doc.children
                    .push(Node::Comment(self.until("-->", "comment")?.into()));
            } else if self.starts_with("<?") {
                doc.children.push(self.processing_instruction()?);
            } else if self.starts_with("<!") {
                return self.fail("document type declarations are not supported");
            } else if self.starts_with("<") {
                if root_seen {
                    return self.fail("extra content at the end of the document");
                }
                root_seen = true;
                doc.children
                    .push(Node::Element(self.element(1, Space::Initial)?));
            } else {
                return self.fail("text outside the root element");
            }
        }
        if !root_seen {
            return Err(ParseError("document is empty".into()));
        }
        Ok(doc)
    }

    fn xml_declaration(&mut self, doc: &mut Document) -> ParseResult<()> {
        loop {
            self.skip_space();
            if self.eat("?>") {
                return Ok(());
            }
            let key = self.name()?;
            self.skip_space();
            if !self.eat("=") {
                return self.fail("expected '=' in the XML declaration");
            }
            self.skip_space();
            let value = self.quoted()?.to_string();
            match key.as_str() {
                "version" => doc.version = value,
                "encoding" => doc.encoding = Some(value),
                "standalone" => doc.standalone = Some(value == "yes"),
                _ => return self.fail("unknown XML declaration attribute"),
            }
        }
    }

    fn quoted(&mut self) -> ParseResult<&'a str> {
        let quote = match self.rest().chars().next() {
            Some(q @ ('"' | '\'')) => q,
            _ => return self.fail("expected a quoted value"),
        };
        self.pos += 1;
        self.until(&quote.to_string(), "quoted value")
    }

    fn processing_instruction(&mut self) -> ParseResult<Node> {
        self.pos += 2;
        let target = self.name()?;
        let body = self.until("?>", "processing instruction")?;
        let data = body.trim_start_matches(is_xml_space);
        if data.len() == body.len() && !body.is_empty() {
            return self.fail("expected whitespace after the processing instruction target");
        }
        Ok(Node::Pi {
            target,
            data: data.to_string(),
        })
    }

    fn element(&mut self, depth: usize, parent_space: Space) -> ParseResult<Element> {
        if depth > MAX_DEPTH {
            return self.fail("excessive depth in document");
        }
        self.pos += 1; // '<'
        let name = self.name()?;
        let mut element = Element {
            name,
            ns_decls: Vec::new(),
            attrs: Vec::new(),
            children: Vec::new(),
        };
        let mut space = match parent_space {
            Space::TextSeen => Space::Initial,
            inherited => inherited,
        };
        loop {
            let separated = self.skip_space();
            if self.eat("/>") {
                return Ok(element);
            }
            if self.eat(">") {
                break;
            }
            if !separated {
                return self.fail("attributes must be separated by whitespace");
            }
            let key = self.name()?;
            self.skip_space();
            if !self.eat("=") {
                return self.fail("expected '=' after an attribute name");
            }
            self.skip_space();
            let value = self.attribute_value()?;
            let seen = element
                .ns_decls
                .iter()
                .chain(&element.attrs)
                .any(|(k, _)| *k == key);
            if seen {
                return self.fail("duplicate attribute");
            }
            if key == "xml:space" {
                match value.as_str() {
                    "preserve" => space = Space::Preserve,
                    "default" => space = Space::Default,
                    _ => {}
                }
            }
            if key == "xmlns" || key.starts_with("xmlns:") {
                // libxml2 reports an error for an empty prefixed declaration and drops it.
                if !(key.starts_with("xmlns:") && value.is_empty()) {
                    element.ns_decls.push((key, value));
                }
            } else {
                element.attrs.push((key, value));
            }
        }
        element.children = self.content(&element.name, depth, space)?;
        Ok(element)
    }

    fn attribute_value(&mut self) -> ParseResult<String> {
        let quote = match self.rest().chars().next() {
            Some(q @ ('"' | '\'')) => q,
            _ => return self.fail("expected a quoted attribute value"),
        };
        self.pos += 1;
        let mut value = String::new();
        loop {
            let Some(c) = self.rest().chars().next() else {
                return self.fail("unterminated attribute value");
            };
            match c {
                c if c == quote => {
                    self.pos += 1;
                    return Ok(value);
                }
                '<' => return self.fail("'<' in an attribute value"),
                '&' => value.push(self.reference()?),
                // Literal whitespace is normalised to a space; character references are kept.
                '\n' | '\t' => {
                    value.push(' ');
                    self.pos += 1;
                }
                c => {
                    value.push(c);
                    self.pos += c.len_utf8();
                }
            }
        }
    }

    /// `&lt;`, `&#65;` and the like, at the current position.
    fn reference(&mut self) -> ParseResult<char> {
        let start = self.pos;
        self.pos += 1;
        let rest = self.rest();
        let Some(end) = rest.find(';') else {
            self.pos = start;
            return self.fail("unterminated reference");
        };
        let body = &rest[..end];
        self.pos += end + 1;
        let decoded = match body {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => {
                let code = if let Some(hex) = body.strip_prefix("#x") {
                    u32::from_str_radix(hex, 16).ok()
                } else if let Some(dec) = body.strip_prefix('#') {
                    dec.parse().ok()
                } else {
                    None
                };
                code.and_then(char::from_u32).filter(|c| is_xml_char(*c))
            }
        };
        match decoded {
            Some(c) => Ok(c),
            None => {
                self.pos = start;
                self.fail("unknown or invalid reference")
            }
        }
    }

    fn content(&mut self, name: &str, depth: usize, mut space: Space) -> ParseResult<Vec<Node>> {
        let mut children: Vec<Node> = Vec::new();
        loop {
            if self.eof() {
                return self.fail("premature end of data");
            }
            if self.eat("</") {
                let closing = self.name()?;
                if closing != name {
                    return self.fail("opening and ending tag mismatch");
                }
                self.skip_space();
                if !self.eat(">") {
                    return self.fail("expected '>' in the end tag");
                }
                return Ok(children);
            } else if self.eat("<!--") {
                children.push(Node::Comment(self.until("-->", "comment")?.into()));
            } else if self.eat("<![CDATA[") {
                push_cdata(&mut children, self.until("]]>", "CDATA section")?);
            } else if self.starts_with("<?") {
                children.push(self.processing_instruction()?);
            } else if self.starts_with("<") {
                children.push(Node::Element(self.element(depth + 1, space)?));
            } else if self.starts_with("&") {
                let c = self.reference()?;
                push_text(&mut children, c.encode_utf8(&mut [0; 4]));
            } else {
                let rest = self.rest();
                let len = rest.find(['<', '&']).unwrap_or(rest.len());
                let run = &rest[..len];
                self.pos += len;
                if run.contains("]]>") {
                    return self.fail("']]>' in character data");
                }
                // libxml2 handles the ASCII prefix of a run and the rest from the first
                // non-ASCII character on separately, with different bookkeeping.
                let split = run.find(|c: char| !c.is_ascii()).unwrap_or(run.len());
                let (prefix, tail) = run.split_at(split);
                if !prefix.is_empty() {
                    let all_blank = prefix.chars().all(is_xml_space);
                    let drop = all_blank
                        && tail.is_empty()
                        && self.starts_with("<")
                        && is_ignorable(space, &children, self.starts_with("</"));
                    if !drop {
                        push_text(&mut children, prefix);
                        // Kept text that starts with whitespace makes later whitespace significant.
                        if prefix.starts_with(is_xml_space) && space == Space::Initial {
                            space = Space::TextSeen;
                        }
                    }
                }
                if !tail.is_empty() {
                    push_text(&mut children, tail);
                    if space == Space::Initial {
                        space = Space::TextSeen;
                    }
                }
            }
        }
    }
}

/// libxml2's `xml:space` state while parsing one element's content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Space {
    /// No `xml:space` in scope and no text seen yet.
    Initial,
    /// Text of a kind that makes all later whitespace in the element significant has been seen.
    TextSeen,
    Default,
    Preserve,
}

/// libxml2's `areBlanks()` heuristic (no DTD): is this whitespace between markup dropped?
/// `children` are those of the current element so far.
fn is_ignorable(space: Space, children: &[Node], before_end_tag: bool) -> bool {
    if matches!(space, Space::Preserve | Space::TextSeen) || (children.is_empty() && before_end_tag)
    {
        return false;
    }
    match children.last() {
        None => true,
        Some(Node::Text(_)) => false,
        Some(_) => !matches!(children.first(), Some(Node::Text(_))),
    }
}

/// Text adjacent to text is one node, as in the libxml2 tree.
fn push_text(children: &mut Vec<Node>, text: &str) {
    match children.last_mut() {
        Some(Node::Text(existing)) => existing.push_str(text),
        _ => children.push(Node::Text(text.to_string())),
    }
}

/// Adjacent CDATA sections are merged too.
fn push_cdata(children: &mut Vec<Node>, text: &str) {
    match children.last_mut() {
        Some(Node::CData(existing)) => existing.push_str(text),
        _ => children.push(Node::CData(text.to_string())),
    }
}

struct Writer {
    out: String,
    /// Without a declared encoding libxml2 writes non-ASCII text as character references.
    escape_non_ascii: bool,
}

fn serialize(doc: &Document) -> String {
    let mut w = Writer {
        out: String::new(),
        escape_non_ascii: doc.encoding.is_none(),
    };
    w.out.push_str("<?xml version=\"");
    w.out.push_str(&doc.version);
    w.out.push('"');
    if let Some(encoding) = &doc.encoding {
        w.out.push_str(" encoding=\"");
        w.out.push_str(encoding);
        w.out.push('"');
    }
    if let Some(standalone) = doc.standalone {
        w.out.push_str(if standalone {
            " standalone=\"yes\""
        } else {
            " standalone=\"no\""
        });
    }
    w.out.push_str("?>\n");
    for node in &doc.children {
        w.node(node, 0, true);
        w.out.push('\n');
    }
    w.out
}

impl Writer {
    fn indent(&mut self, level: usize) {
        for _ in 0..2 * level.min(MAX_INDENT_LEVELS) {
            self.out.push(' ');
        }
    }

    fn node(&mut self, node: &Node, level: usize, format: bool) {
        match node {
            Node::Element(e) => self.element(e, level, format),
            Node::Text(t) => self.escaped(t, false),
            Node::CData(c) => {
                self.out.push_str("<![CDATA[");
                self.out.push_str(c);
                self.out.push_str("]]>");
            }
            Node::Comment(c) => {
                self.out.push_str("<!--");
                self.out.push_str(c);
                self.out.push_str("-->");
            }
            Node::Pi { target, data } => {
                self.out.push_str("<?");
                self.out.push_str(target);
                if !data.is_empty() {
                    self.out.push(' ');
                    self.out.push_str(data);
                }
                self.out.push_str("?>");
            }
        }
    }

    fn element(&mut self, e: &Element, level: usize, format: bool) {
        self.out.push('<');
        self.out.push_str(&e.name);
        for (key, value) in &e.ns_decls {
            self.out.push(' ');
            self.out.push_str(key);
            self.out.push('=');
            self.namespace_value(value);
        }
        for (key, value) in &e.attrs {
            self.out.push(' ');
            self.out.push_str(key);
            self.out.push_str("=\"");
            self.escaped(value, true);
            self.out.push('"');
        }
        if e.children.is_empty() {
            self.out.push_str("/>");
            return;
        }
        self.out.push('>');
        // An element with a text child is written without indentation, including everything below it.
        let format = format
            && !e
                .children
                .iter()
                .any(|c| matches!(c, Node::Text(_) | Node::CData(_)));
        if format {
            self.out.push('\n');
        }
        for child in &e.children {
            if format {
                self.indent(level + 1);
            }
            self.node(child, level + 1, format);
            if format {
                self.out.push('\n');
            }
        }
        if format {
            self.indent(level);
        }
        self.out.push_str("</");
        self.out.push_str(&e.name);
        self.out.push('>');
    }

    /// libxml2 writes namespace URIs differently from attribute values: only `&` is escaped,
    /// and the quote character is chosen to fit the value.
    fn namespace_value(&mut self, value: &str) {
        let value = value.replace('&', "&#38;");
        if !value.contains('"') {
            self.out.push('"');
            self.out.push_str(&value);
            self.out.push('"');
        } else if !value.contains('\'') {
            self.out.push('\'');
            self.out.push_str(&value);
            self.out.push('\'');
        } else {
            self.out.push('"');
            self.out.push_str(&value.replace('"', "&quot;"));
            self.out.push('"');
        }
    }

    fn escaped(&mut self, s: &str, attribute: bool) {
        for c in s.chars() {
            match c {
                '&' => self.out.push_str("&amp;"),
                '<' => self.out.push_str("&lt;"),
                '>' => self.out.push_str("&gt;"),
                '\r' if attribute => self.out.push_str("&#13;"),
                // libxml2 uses a different escaper when there is no declared encoding.
                '\r' if self.escape_non_ascii => self.out.push_str("&#xD;"),
                '\r' => self.out.push_str("&#13;"),
                '"' if attribute => self.out.push_str("&quot;"),
                '\n' if attribute => self.out.push_str("&#10;"),
                '\t' if attribute => self.out.push_str("&#9;"),
                c if !c.is_ascii() && self.escape_non_ascii => {
                    self.out.push_str(&format!("&#x{:X};", c as u32));
                }
                c => self.out.push(c),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(xml: &str) -> String {
        try_format(xml.as_bytes()).unwrap_or_else(|e| panic!("{xml}: {e:?}"))
    }

    // The expectations below were produced by xmllint (libxml 2.9.13).

    #[test]
    fn indents_elements_and_declares_the_document() {
        assert_eq!(
            fmt("<a>\n  <b>1</b>\n  <c>\n    <d/>\n  </c>\n</a>\n"),
            "<?xml version=\"1.0\"?>\n<a>\n  <b>1</b>\n  <c>\n    <d/>\n  </c>\n</a>\n"
        );
    }

    #[test]
    fn declaration_keeps_encoding_and_standalone() {
        assert_eq!(
            fmt("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><a/>"),
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<a/>\n"
        );
    }

    #[test]
    fn non_ascii_is_escaped_only_without_a_declared_encoding() {
        assert_eq!(fmt("<a>é</a>"), "<?xml version=\"1.0\"?>\n<a>&#xE9;</a>\n");
        assert_eq!(
            fmt("<?xml version=\"1.0\" encoding=\"UTF-8\"?><a>é😀</a>"),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<a>é😀</a>\n"
        );
    }

    #[test]
    fn comments_and_processing_instructions_are_kept_in_place() {
        assert_eq!(
            fmt("<?xml version=\"1.0\"?><!-- pre --><?pi data?><a><?inner x y?></a><!-- post -->"),
            "<?xml version=\"1.0\"?>\n<!-- pre -->\n<?pi data?>\n<a>\n  <?inner x y?>\n</a>\n<!-- post -->\n"
        );
    }

    #[test]
    fn entities_and_character_references_are_normalised() {
        assert_eq!(
            fmt("<a>1 &lt; 2 &gt; &amp; \"q\" &#65; &#x42;</a>"),
            "<?xml version=\"1.0\"?>\n<a>1 &lt; 2 &gt; &amp; \"q\" A B</a>\n"
        );
        assert_eq!(
            fmt("<a k=\"v&#10;w&#9;x&lt;&amp;&gt;&quot;\"/>"),
            "<?xml version=\"1.0\"?>\n<a k=\"v&#10;w&#9;x&lt;&amp;&gt;&quot;\"/>\n"
        );
    }

    #[test]
    fn whitespace_between_elements_is_dropped_unless_the_element_has_text() {
        let decl = "<?xml version=\"1.0\"?>\n";
        // Dropped while no text has been seen, text later makes the element mixed content.
        assert_eq!(
            fmt("<a><b>x</b>  <c/> text <d/></a>"),
            format!("{decl}<a><b>x</b><c/> text <d/></a>\n")
        );
        assert_eq!(
            fmt("<a>x<b/>  <c/></a>"),
            format!("{decl}<a>x<b/>  <c/></a>\n")
        );
        assert_eq!(
            fmt("<a><b/>  <c/>x</a>"),
            format!("{decl}<a><b/><c/>x</a>\n")
        );
        assert_eq!(fmt("<a>  <b/>  </a>"), format!("{decl}<a>\n  <b/>\n</a>\n"));
    }

    #[test]
    fn whitespace_only_content_is_kept() {
        let decl = "<?xml version=\"1.0\"?>\n";
        assert_eq!(fmt("<a>\n</a>"), format!("{decl}<a>\n</a>\n"));
        assert_eq!(
            fmt("<a><b>\n</b></a>"),
            format!("{decl}<a>\n  <b>\n</b>\n</a>\n")
        );
        assert_eq!(
            fmt("<a><e>  </e></a>"),
            format!("{decl}<a>\n  <e>  </e>\n</a>\n")
        );
    }

    #[test]
    fn xml_space_preserve_keeps_whitespace() {
        assert_eq!(
            fmt("<a xml:space=\"preserve\"> <b/> </a>"),
            "<?xml version=\"1.0\"?>\n<a xml:space=\"preserve\"> <b/> </a>\n"
        );
    }

    #[test]
    fn namespace_declarations_come_before_attributes() {
        assert_eq!(
            fmt(
                "<p:a k2=\"2\" xmlns:p=\"urn:p\" xmlns=\"urn:d\" p:k=\"1\"><p:b/><c xmlns=\"urn:e\"/></p:a>"
            ),
            "<?xml version=\"1.0\"?>\n<p:a xmlns:p=\"urn:p\" xmlns=\"urn:d\" k2=\"2\" p:k=\"1\">\n  <p:b/>\n  <c xmlns=\"urn:e\"/>\n</p:a>\n"
        );
    }

    #[test]
    fn cdata_is_kept() {
        assert_eq!(
            fmt("<a><b>x</b><![CDATA[ ]]><c/></a>"),
            "<?xml version=\"1.0\"?>\n<a><b>x</b><![CDATA[ ]]><c/></a>\n"
        );
    }

    #[test]
    fn line_ends_are_normalised_and_literal_attribute_whitespace_becomes_a_space() {
        assert_eq!(
            fmt("<a>x\r\ny</a>"),
            "<?xml version=\"1.0\"?>\n<a>x\ny</a>\n"
        );
        assert_eq!(
            fmt("<a k=\"p\nq\tr\"/>"),
            "<?xml version=\"1.0\"?>\n<a k=\"p q r\"/>\n"
        );
    }

    #[test]
    fn indentation_stops_growing_after_100_levels() {
        let deep = 105;
        let xml = format!("{}{}", "<a>".repeat(deep), "</a>".repeat(deep));
        let out = fmt(&xml);
        let longest = out
            .lines()
            .map(|l| l.len() - l.trim_start().len())
            .max()
            .unwrap();
        assert_eq!(longest, 200);
    }

    #[test]
    fn malformed_documents_are_errors() {
        for bad in [
            "",
            "   ",
            "<a><b></a>",
            "<a>",
            "<a/><b/>",
            "text",
            "<a b=1/>",
            "<a b=\"1\" b=\"2\"/>",
            "<a>&nope;</a>",
            "<a>&#0;</a>",
            "<a b=\"<\"/>",
            "<!DOCTYPE a><a/>",
            "<a>]]></a>",
        ] {
            assert!(try_format(bad.as_bytes()).is_err(), "{bad:?}");
        }
        assert!(try_format(b"\xff<a/>").is_err());
        let too_deep = format!("{}{}", "<a>".repeat(257), "</a>".repeat(257));
        assert!(try_format(too_deep.as_bytes()).is_err());
    }
}
