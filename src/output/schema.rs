//! Extraction of the `<data>` text from a `get-schema` reply.
//!
//! May depend on: error.

use roxmltree::{Document, Node};

use crate::protocol::ns::NC_NS;

/// The schema text of a `get-schema` reply, or the message to show when the reply is not as expected.
pub fn extract(reply: &[u8]) -> Result<String, &'static str> {
    const UNPARSABLE: &str = "Couldn't parse the RPC reply";
    let text = std::str::from_utf8(reply).map_err(|_| UNPARSABLE)?;
    let doc = Document::parse(text).map_err(|_| UNPARSABLE)?;
    let root = doc.root_element();

    // Like the Python `strip()`: only elements and text that is not blank count.
    let meaningful = |n: &Node| n.is_element() || n.text().is_some_and(|t| !t.trim().is_empty());
    let is_reply =
        root.tag_name().namespace() == Some(NC_NS) && root.tag_name().name() == "rpc-reply";
    let first = root.children().find(meaningful).filter(|_| is_reply);
    let Some(first) = first else {
        return Err("Didn't get rpc-reply");
    };
    if first.is_element()
        && first.tag_name().name() == "data"
        && let Some(text) = first.first_child().and_then(|n| n.text())
    {
        return Ok(text.to_string());
    }
    Err("Didn't get data in the reply")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(inner: &str) -> Vec<u8> {
        format!("<rpc-reply xmlns=\"{NC_NS}\" message-id=\"1\">{inner}</rpc-reply>").into_bytes()
    }

    #[test]
    fn returns_the_decoded_text_of_data() {
        let msg = reply("\n  <data>module m { }\n&lt;x&gt; &amp; y</data>\n");
        assert_eq!(extract(&msg).unwrap(), "module m { }\n<x> & y");
    }

    #[test]
    fn cdata_works_too() {
        assert_eq!(
            extract(&reply("<data><![CDATA[a < b]]></data>")).unwrap(),
            "a < b"
        );
    }

    #[test]
    fn problems_are_reported_with_pythons_messages() {
        assert_eq!(
            extract(&reply("<rpc-error/>")),
            Err("Didn't get data in the reply")
        );
        assert_eq!(
            extract(&reply("<data/>")),
            Err("Didn't get data in the reply")
        );
        assert_eq!(
            extract(&reply("<data><x/></data>")),
            Err("Didn't get data in the reply")
        );
        assert_eq!(extract(&reply("")), Err("Didn't get rpc-reply"));
        assert_eq!(
            extract(b"<hello xmlns=\"urn:x\"><data>t</data></hello>"),
            Err("Didn't get rpc-reply")
        );
        assert_eq!(extract(b"<a>"), Err("Couldn't parse the RPC reply"));
        assert_eq!(extract(b"\xff"), Err("Couldn't parse the RPC reply"));
    }
}
