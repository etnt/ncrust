//! `--rpc`: wraps a user-supplied operation in `<rpc>`.
//!
//! May depend on: config, error, protocol::{ns, xmlw}.

use crate::protocol::ns::NC_NS;

pub const END: &str = "</rpc>";

/// Everything up to the start of the user's operation, which is streamed from a file after it.
pub fn start(message_id: u32, rpc_attribute: &str) -> String {
    let attrs = rpc_attribute.trim();
    let sep = if attrs.is_empty() { "" } else { " " };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rpc xmlns=\"{NC_NS}\" message-id=\"{message_id}\"{sep}{attrs}>"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_tag_carries_id_and_raw_attributes() {
        assert!(start(1, "").ends_with("message-id=\"1\">"));
        assert!(start(1, " a=\"b\" ").ends_with("message-id=\"1\" a=\"b\">"));
    }
}
