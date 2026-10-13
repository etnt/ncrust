//! Operations without payload: `commit`, `discard-changes`, `kill-session`, `validate`,
//! copy running to startup.
//!
//! May depend on: config, error, protocol::{ns, xmlw}.

use super::Operation;
use crate::config::Db;
use crate::protocol::xmlw::XmlWriter;

/// Sent by the tool when it is done (not selectable from the command line).
pub struct CloseSession;

impl Operation for CloseSession {
    fn write_body(&self, w: &mut XmlWriter) {
        w.empty("close-session", &[]);
    }
}

pub struct KillSession(pub String);

impl Operation for KillSession {
    fn write_body(&self, w: &mut XmlWriter) {
        w.start("kill-session", &[]);
        w.element("session-id", &[], &self.0);
        w.end();
    }
}

pub struct DiscardChanges;

impl Operation for DiscardChanges {
    fn write_body(&self, w: &mut XmlWriter) {
        w.empty("discard-changes", &[]);
    }
}

pub struct Commit;

impl Operation for Commit {
    fn write_body(&self, w: &mut XmlWriter) {
        w.empty("commit", &[]);
    }
}

pub struct Validate(pub Db);

impl Operation for Validate {
    fn write_body(&self, w: &mut XmlWriter) {
        w.start("validate", &[]).start("source", &[]);
        w.empty(&self.0.0, &[]);
        w.end().end();
    }
}

pub struct CopyRunningToStartup;

impl Operation for CopyRunningToStartup {
    fn write_body(&self, w: &mut XmlWriter) {
        w.start("copy-config", &[]);
        w.start("target", &[]).empty("startup", &[]).end();
        w.start("source", &[]).empty("running", &[]).end();
        w.end();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ops::rpc;

    fn body(op: &dyn Operation) -> String {
        let msg = rpc(op, 1, "");
        let start = msg
            .find('>')
            .and_then(|i| msg[i + 1..].find('>').map(|j| i + j + 2))
            .unwrap();
        msg[start..msg.len() - "</rpc>".len()].to_string()
    }

    #[test]
    fn bodies() {
        assert_eq!(body(&CloseSession), "<close-session/>");
        assert_eq!(
            body(&KillSession("4<2".into())),
            "<kill-session><session-id>4&lt;2</session-id></kill-session>"
        );
        assert_eq!(body(&DiscardChanges), "<discard-changes/>");
        assert_eq!(body(&Commit), "<commit/>");
        assert_eq!(
            body(&Validate(Db("candidate".into()))),
            "<validate><source><candidate/></source></validate>"
        );
        assert_eq!(
            body(&CopyRunningToStartup),
            "<copy-config><target><startup/></target><source><running/></source></copy-config>"
        );
    }
}
