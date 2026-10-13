//! Operations without payload: `commit`, `discard-changes`, `kill-session`, `validate`,
//! copy running to startup.
//!
//! May depend on: config, error, protocol::{ns, xmlw}.

use super::Operation;
use crate::protocol::xmlw::XmlWriter;

/// Sent by the tool when it is done (not selectable from the command line).
pub struct CloseSession;

impl Operation for CloseSession {
    fn write_body(&self, w: &mut XmlWriter) {
        w.empty("close-session", &[]);
    }
}
