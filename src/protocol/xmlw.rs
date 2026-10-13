//! Small XML writer that escapes text and attribute values.
//!
//! May depend on: error.

const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";

/// Builds an XML document in a string. Text and attribute values are escaped; element and
/// attribute names are trusted (they are constants in the operation builders).
#[derive(Debug, Default)]
pub struct XmlWriter {
    out: String,
    open: Vec<String>,
}

impl XmlWriter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn declaration(&mut self) -> &mut Self {
        self.out.push_str(DECLARATION);
        self
    }

    pub fn start(&mut self, name: &str, attrs: &[(&str, &str)]) -> &mut Self {
        self.start_with_raw_attrs(name, attrs, "")
    }

    /// Like `start`, with `raw_attrs` inserted unescaped after the attributes (for `--rpc-attribute`).
    pub fn start_with_raw_attrs(
        &mut self,
        name: &str,
        attrs: &[(&str, &str)],
        raw_attrs: &str,
    ) -> &mut Self {
        self.open_tag(name, attrs, raw_attrs);
        self.out.push('>');
        self.open.push(name.to_string());
        self
    }

    pub fn end(&mut self) -> &mut Self {
        let name = self.open.pop().expect("end() without matching start()");
        self.out.push_str("</");
        self.out.push_str(&name);
        self.out.push('>');
        self
    }

    pub fn empty(&mut self, name: &str, attrs: &[(&str, &str)]) -> &mut Self {
        self.open_tag(name, attrs, "");
        self.out.push_str("/>");
        self
    }

    pub fn text(&mut self, text: &str) -> &mut Self {
        escape_into(&mut self.out, text, false);
        self
    }

    /// Inserts `xml` unescaped. The caller guarantees it is well-formed (payloads read from files).
    pub fn raw(&mut self, xml: &str) -> &mut Self {
        self.out.push_str(xml);
        self
    }

    /// `<name attrs>text</name>`.
    pub fn element(&mut self, name: &str, attrs: &[(&str, &str)], text: &str) -> &mut Self {
        self.start(name, attrs).text(text).end()
    }

    pub fn finish(self) -> String {
        debug_assert!(self.open.is_empty(), "unclosed elements: {:?}", self.open);
        self.out
    }

    fn open_tag(&mut self, name: &str, attrs: &[(&str, &str)], raw_attrs: &str) {
        self.out.push('<');
        self.out.push_str(name);
        for (key, value) in attrs {
            self.out.push(' ');
            self.out.push_str(key);
            self.out.push_str("=\"");
            escape_into(&mut self.out, value, true);
            self.out.push('"');
        }
        let raw_attrs = raw_attrs.trim();
        if !raw_attrs.is_empty() {
            self.out.push(' ');
            self.out.push_str(raw_attrs);
        }
    }
}

fn escape_into(out: &mut String, s: &str, attribute: bool) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attribute => out.push_str("&quot;"),
            // Literal whitespace in attribute values would be normalised to spaces by parsers.
            '\n' if attribute => out.push_str("&#10;"),
            '\r' if attribute => out.push_str("&#13;"),
            '\t' if attribute => out.push_str("&#9;"),
            _ => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_text_and_attributes() {
        let mut w = XmlWriter::new();
        w.start("a", &[("k", "x\"<&>'\n")])
            .text("1 < 2 && 3 > 2")
            .end();
        assert_eq!(
            w.finish(),
            "<a k=\"x&quot;&lt;&amp;&gt;'&#10;\">1 &lt; 2 &amp;&amp; 3 &gt; 2</a>"
        );
    }

    #[test]
    fn builds_nested_and_empty_elements() {
        let mut w = XmlWriter::new();
        w.declaration().start("a", &[]);
        w.empty("b", &[("x", "1")]).element("c", &[], "t");
        w.end();
        assert_eq!(
            w.finish(),
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<a><b x=\"1\"/><c>t</c></a>"
        );
    }

    #[test]
    fn raw_attributes_and_raw_content_are_not_escaped() {
        let mut w = XmlWriter::new();
        w.start_with_raw_attrs("a", &[("k", "v")], "  x=\"1\" y='2' ")
            .raw("<b/>")
            .end();
        assert_eq!(w.finish(), "<a k=\"v\" x=\"1\" y='2'><b/></a>");
    }
}
