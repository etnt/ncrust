//! Helpers shared by integration tests.
#![allow(dead_code)]

pub mod mock_server;

use std::io::Write;

use tempfile::NamedTempFile;

/// Normal form for comparing XML: expanded names (prefixes ignored), sorted attributes,
/// no whitespace-only text, no comments or processing instructions.
pub fn canonical_xml(xml: &str) -> Result<String, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("invalid XML: {e}"))?;
    let mut out = String::new();
    canonical_node(doc.root(), &mut out);
    Ok(out)
}

fn canonical_node(node: roxmltree::Node, out: &mut String) {
    match node.node_type() {
        roxmltree::NodeType::Element => {
            let name = node.tag_name();
            let mut attrs: Vec<_> = node
                .attributes()
                .map(|a| (a.namespace().unwrap_or(""), a.name(), a.value()))
                .collect();
            attrs.sort();
            out.push_str(&format!(
                "<{{{}}}{}",
                name.namespace().unwrap_or(""),
                name.name()
            ));
            for (ns, local, value) in attrs {
                out.push_str(&format!(" {{{ns}}}{local}={value:?}"));
            }
            out.push('>');
            node.children().for_each(|c| canonical_node(c, out));
            out.push_str("</>");
        }
        roxmltree::NodeType::Text => {
            let text = node.text().unwrap_or("").trim();
            if !text.is_empty() {
                out.push_str(&format!("{text:?}"));
            }
        }
        roxmltree::NodeType::Root => node.children().for_each(|c| canonical_node(c, out)),
        _ => {}
    }
}

/// Panics unless both documents are equal after canonicalisation.
#[track_caller]
pub fn assert_xml_eq(expected: &str, actual: &str) {
    let e = canonical_xml(expected).unwrap_or_else(|m| panic!("expected side: {m}"));
    let a = canonical_xml(actual).unwrap_or_else(|m| panic!("actual side: {m}"));
    assert_eq!(e, a, "XML differs\nexpected: {expected}\nactual: {actual}");
}

/// Temporary file holding `contents`; removed when dropped.
pub fn temp_file_with(contents: &[u8]) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("create temp file");
    file.write_all(contents).expect("write temp file");
    file.flush().expect("flush temp file");
    file
}
