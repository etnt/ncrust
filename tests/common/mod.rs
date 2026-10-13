//! Helpers shared by integration tests.
#![allow(dead_code)]

pub mod mock_server;
#[cfg(feature = "russh")]
pub mod ssh_server;

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

/// An invocation against the mock server's TCP port.
pub fn tcp_invocation(
    port: u16,
    action: ncrust::config::Action,
    style: Option<ncrust::config::OutputStyle>,
    iterations: i64,
) -> ncrust::config::Invocation {
    use ncrust::config::{ConnectionSpec, Invocation, TransportKind};
    Invocation {
        connection: ConnectionSpec {
            host: "127.0.0.1".into(),
            port,
            username: "admin".into(),
            kind: TransportKind::Tcp {
                groups: String::new(),
                sup_gids: String::new(),
            },
            call_home: None,
        },
        action,
        style,
        iterations,
        version: None,
        rpc_attribute: String::new(),
        debug: false,
    }
}

/// Runs the invocation; returns the result, what went to stdout and what went to stderr.
pub fn run_capture(
    invocation: &ncrust::config::Invocation,
) -> (Result<(), ncrust::error::Error>, String, String) {
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let result = ncrust::runner::run(invocation, &mut out, &mut err);
    (
        result,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

/// Temporary file holding `contents`; removed when dropped.
pub fn temp_file_with(contents: &[u8]) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("create temp file");
    file.write_all(contents).expect("write temp file");
    file.flush().expect("flush temp file");
    file
}
