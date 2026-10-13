//! Builds the `<filter>` and stream-filter elements from a `config::Filter`.
//!
//! May depend on: config, error, protocol::xmlw.

use crate::protocol::xmlw::XmlWriter;

/// A filter whose content has already been read (a subtree filter comes from a file).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedFilter {
    XPath(String),
    /// Well-formed XML, inserted as is.
    Subtree(String),
}

impl ResolvedFilter {
    pub fn write(&self, w: &mut XmlWriter) {
        match self {
            ResolvedFilter::XPath(select) => {
                w.empty("filter", &[("type", "xpath"), ("select", select)]);
            }
            ResolvedFilter::Subtree(xml) => {
                w.start("filter", &[("type", "subtree")]).raw(xml).end();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn written(filter: &ResolvedFilter) -> String {
        let mut w = XmlWriter::new();
        filter.write(&mut w);
        w.finish()
    }

    #[test]
    fn xpath_with_either_kind_of_quote_is_escaped() {
        assert_eq!(
            written(&ResolvedFilter::XPath("/a[b='c' and d=\"e\"]".into())),
            "<filter type=\"xpath\" select=\"/a[b='c' and d=&quot;e&quot;]\"/>"
        );
    }

    #[test]
    fn subtree_content_is_inserted_unchanged() {
        assert_eq!(
            written(&ResolvedFilter::Subtree("<a><b/></a>".into())),
            "<filter type=\"subtree\"><a><b/></a></filter>"
        );
    }
}
