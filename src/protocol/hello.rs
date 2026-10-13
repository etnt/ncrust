//! Client hello construction and server hello parsing (capabilities, framing choice).
//!
//! May depend on: config, error, protocol::{ns, xmlw}.

use crate::config::NetconfVersion;
use crate::error::Error;
use crate::protocol::ns::{BASE_1_0, BASE_1_1, NC_NS};
use crate::protocol::xmlw::XmlWriter;

/// The client `<hello>` advertising the base capabilities of `versions`.
pub fn client_hello(versions: &[NetconfVersion]) -> String {
    let mut w = XmlWriter::new();
    w.declaration()
        .start("hello", &[("xmlns", NC_NS)])
        .start("capabilities", &[]);
    for version in versions {
        let uri = match version {
            NetconfVersion::V1_0 => BASE_1_0,
            NetconfVersion::V1_1 => BASE_1_1,
        };
        w.element("capability", &[], uri);
    }
    w.end().end();
    w.finish()
}

/// Capability URIs of a server `<hello>`. A well-formed document of another shape yields no
/// capabilities (the session then stays on base:1.0), as the Python script does.
pub fn parse_server_hello(xml: &[u8]) -> Result<Vec<String>, Error> {
    let text = std::str::from_utf8(xml)
        .map_err(|e| Error::Stderr(format!("Couldn't parse the NETCONF hello message: {e}")))?;
    let doc = roxmltree::Document::parse(text)
        .map_err(|e| Error::Stderr(format!("Couldn't parse the NETCONF hello message: {e}")))?;

    let in_netconf_ns = |n: &roxmltree::Node, name: &str| {
        n.is_element() && n.tag_name().namespace() == Some(NC_NS) && n.tag_name().name() == name
    };
    let root = doc.root_element();
    if !in_netconf_ns(&root, "hello") {
        return Ok(Vec::new());
    }
    Ok(root
        .children()
        .filter(|n| in_netconf_ns(n, "capabilities"))
        .flat_map(|caps| caps.children())
        .filter(|n| in_netconf_ns(n, "capability"))
        .filter_map(|n| n.text())
        .map(|t| t.trim().to_string())
        .collect())
}

/// Base version to use: 1.1 only if we offered it and the server advertises it.
pub fn negotiate(offered: &[NetconfVersion], server_capabilities: &[String]) -> NetconfVersion {
    let server_has_1_1 = server_capabilities.iter().any(|c| c == BASE_1_1);
    if offered.contains(&NetconfVersion::V1_1) && server_has_1_1 {
        NetconfVersion::V1_1
    } else {
        NetconfVersion::V1_0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use NetconfVersion::{V1_0, V1_1};

    fn server_hello(caps: &[&str]) -> String {
        let items: String = caps
            .iter()
            .map(|c| format!("<capability>{c}</capability>"))
            .collect();
        format!(
            "<?xml version=\"1.0\"?><hello xmlns=\"{NC_NS}\"><capabilities>{items}</capabilities>\
             <session-id>7</session-id></hello>"
        )
    }

    #[test]
    fn client_hello_lists_requested_versions_only() {
        let both = client_hello(&[V1_0, V1_1]);
        assert!(both.contains(&format!("<capability>{BASE_1_0}</capability>")));
        assert!(both.contains(&format!("<capability>{BASE_1_1}</capability>")));
        let only_11 = client_hello(&[V1_1]);
        assert!(!only_11.contains(BASE_1_0) && only_11.contains(BASE_1_1));
    }

    #[test]
    fn parses_capabilities_and_trims_whitespace() {
        let xml = format!(
            "<hello xmlns=\"{NC_NS}\"><session-id>1</session-id><capabilities>\n  \
             <capability>\n   {BASE_1_0}\n  </capability><capability>urn:x</capability>\n\
             </capabilities></hello>"
        );
        assert_eq!(
            parse_server_hello(xml.as_bytes()).unwrap(),
            [BASE_1_0, "urn:x"]
        );
    }

    #[test]
    fn other_document_shapes_yield_no_capabilities() {
        assert!(
            parse_server_hello(b"<rpc-reply xmlns=\"x\"/>")
                .unwrap()
                .is_empty()
        );
        let wrong_ns = format!(
            "<hello xmlns=\"urn:other\"><capabilities><capability>{BASE_1_1}</capability></capabilities></hello>"
        );
        assert!(parse_server_hello(wrong_ns.as_bytes()).unwrap().is_empty());
    }

    #[test]
    fn malformed_hello_is_an_error() {
        for bad in [&b""[..], b"<hello>", b"not xml", b"\xff\xfe"] {
            let err = parse_server_hello(bad).unwrap_err();
            assert!(
                err.to_string()
                    .starts_with("Couldn't parse the NETCONF hello message"),
                "{err}"
            );
        }
    }

    #[test]
    fn negotiation_table() {
        let both = server_caps(&[BASE_1_0, BASE_1_1]);
        let only_10 = server_caps(&[BASE_1_0]);
        let cases = [
            (&[V1_0, V1_1][..], &both, V1_1),
            (&[V1_0, V1_1], &only_10, V1_0),
            (&[V1_0], &both, V1_0),
            (&[V1_1], &both, V1_1),
            // Python keeps framing 1.0 here instead of failing.
            (&[V1_1], &only_10, V1_0),
            (&[V1_0, V1_1], &Vec::new(), V1_0),
        ];
        for (offered, caps, expected) in cases {
            assert_eq!(
                negotiate(offered, caps),
                expected,
                "{offered:?} vs {caps:?}"
            );
        }
    }

    fn server_caps(caps: &[&str]) -> Vec<String> {
        parse_server_hello(server_hello(caps).as_bytes()).unwrap()
    }
}
