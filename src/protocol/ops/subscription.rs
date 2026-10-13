//! `create-subscription`, `establish-subscription` and `kill-subscription`.
//!
//! May depend on: config, error, protocol::{filter, ns, xmlw}.

use super::Operation;
use crate::config::UtcTimestamp;
use crate::protocol::filter::ResolvedFilter;
use crate::protocol::ns::{NOTIFICATION_NS, SUBSCRIBED_NOTIFICATIONS_NS};
use crate::protocol::xmlw::XmlWriter;

/// RFC 5277 `create-subscription`.
pub struct CreateSubscription {
    pub stream: String,
    pub filter: Option<ResolvedFilter>,
    pub start_time: Option<UtcTimestamp>,
    pub stop_time: Option<UtcTimestamp>,
}

impl Operation for CreateSubscription {
    fn write_body(&self, w: &mut XmlWriter) {
        w.start("create-subscription", &[("xmlns", NOTIFICATION_NS)]);
        w.element("stream", &[], &self.stream);
        if let Some(filter) = &self.filter {
            filter.write(w);
        }
        if let Some(start) = &self.start_time {
            w.element("startTime", &[], &format!("{}+00:00", start.as_str()));
        }
        if let Some(stop) = &self.stop_time {
            w.element("stopTime", &[], &format!("{}+00:00", stop.as_str()));
        }
        w.end();
    }
}

/// RFC 8639 `establish-subscription`.
pub struct EstablishSubscription {
    pub stream: String,
    pub filter: Option<ResolvedFilter>,
    pub replay_start_time: Option<UtcTimestamp>,
    pub stop_time: Option<UtcTimestamp>,
}

impl Operation for EstablishSubscription {
    fn write_body(&self, w: &mut XmlWriter) {
        w.start(
            "establish-subscription",
            &[("xmlns", SUBSCRIBED_NOTIFICATIONS_NS)],
        );
        let ns = [("xmlns", SUBSCRIBED_NOTIFICATIONS_NS)];
        match &self.filter {
            Some(ResolvedFilter::XPath(xpath)) => {
                w.element("stream-xpath-filter", &ns, xpath);
            }
            Some(ResolvedFilter::Subtree(xml)) => {
                w.start("stream-subtree-filter", &ns).raw(xml).end();
            }
            None => {}
        }
        w.element("stream", &[], &self.stream);
        if let Some(start) = &self.replay_start_time {
            w.element(
                "replay-start-time",
                &[],
                &format!("{}+00:00", start.as_str()),
            );
        }
        if let Some(stop) = &self.stop_time {
            w.element("stop-time", &[], &format!("{}+00:00", stop.as_str()));
        }
        w.end();
    }
}

pub struct KillSubscription(pub u32);

impl Operation for KillSubscription {
    fn write_body(&self, w: &mut XmlWriter) {
        w.start(
            "kill-subscription",
            &[("xmlns", SUBSCRIBED_NOTIFICATIONS_NS)],
        );
        w.element("id", &[], &self.0.to_string());
        w.end();
    }
}
