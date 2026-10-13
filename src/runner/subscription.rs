//! Subscription handling: stop time and `replayComplete` detection.
//!
//! May depend on: config, error.

use std::time::{SystemTime, UNIX_EPOCH};

/// Whether `message` is a notification announcing the end of the replay (RFC 5277 `replayComplete`).
pub fn is_replay_complete(message: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(message) else {
        return false;
    };
    let Ok(doc) = roxmltree::Document::parse(text) else {
        return false;
    };
    let root = doc.root_element();
    root.tag_name().name() == "notification"
        && root
            .descendants()
            .any(|n| n.is_element() && n.tag_name().name() == "replayComplete")
}

pub fn now_epoch_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// A subscription with a stop time ends client-side once the replay is complete and the stop
/// time has passed. Without a stop time the server decides.
pub fn stop_reached(stop_time: Option<i64>, replay_complete: bool, now: i64) -> bool {
    stop_time.is_some_and(|stop| replay_complete && now >= stop)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_replay_complete_notifications_in_any_namespace_form() {
        let plain = b"<notification xmlns=\"urn:n\"><eventTime>t</eventTime><replayComplete xmlns=\"urn:n\"/></notification>";
        let prefixed = b"<n:notification xmlns:n=\"urn:n\">\n  <n:eventTime>t</n:eventTime>\n  <n:replayComplete/>\n</n:notification>";
        assert!(is_replay_complete(plain));
        assert!(is_replay_complete(prefixed));
    }

    #[test]
    fn other_messages_are_not_replay_complete() {
        assert!(!is_replay_complete(
            b"<notification xmlns=\"urn:n\"><linkUp/></notification>"
        ));
        assert!(!is_replay_complete(
            b"<rpc-reply><replayComplete/></rpc-reply>"
        ));
        assert!(!is_replay_complete(b"not xml"));
        assert!(!is_replay_complete(b"\xff"));
    }

    #[test]
    fn stop_needs_a_stop_time_a_finished_replay_and_a_passed_deadline() {
        assert!(!stop_reached(None, true, 100));
        assert!(!stop_reached(Some(50), false, 100));
        assert!(!stop_reached(Some(150), true, 100));
        assert!(stop_reached(Some(100), true, 100));
    }
}
