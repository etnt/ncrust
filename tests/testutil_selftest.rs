mod common;

use common::{assert_xml_eq, canonical_xml, temp_file_with};

#[test]
fn ignores_whitespace_prefixes_and_attribute_order() {
    let a = r#"<?xml version="1.0"?>
<rpc xmlns="urn:a" message-id="1" x="2">
    <get/>
</rpc>"#;
    let b = r#"<n:rpc xmlns:n="urn:a" x="2" message-id="1"><n:get/></n:rpc>"#;
    assert_xml_eq(a, b);
}

#[test]
fn detects_namespace_text_and_attribute_differences() {
    let base = r#"<a xmlns="urn:a" k="v">t</a>"#;
    for other in [
        r#"<a xmlns="urn:b" k="v">t</a>"#,
        r#"<a xmlns="urn:a" k="w">t</a>"#,
        r#"<a xmlns="urn:a" k="v">u</a>"#,
        r#"<a xmlns="urn:a" k="v"><b/></a>"#,
    ] {
        assert_ne!(canonical_xml(base), canonical_xml(other), "{other}");
    }
}

#[test]
fn reports_malformed_xml() {
    assert!(
        canonical_xml("<a><b></a>")
            .unwrap_err()
            .starts_with("invalid XML")
    );
}

#[test]
fn temp_file_holds_contents() {
    let file = temp_file_with(b"<config/>");
    assert_eq!(std::fs::read(file.path()).unwrap(), b"<config/>");
}
